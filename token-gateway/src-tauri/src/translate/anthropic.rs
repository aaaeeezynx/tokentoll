//! OpenAI Chat Completions 回包 → Anthropic Messages（非流式與流式）。

use super::*;
use serde_json::json;

fn openai_finish_to_anthropic(finish: &str) -> &'static str {
    match finish {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        "content_filter" => "end_turn",
        _ => "end_turn",
    }
}

/// OpenAI 非流式回包 → Anthropic message。echo_model 原樣回顯請求模型。
pub(crate) fn openai_to_anthropic(resp: &Value, echo_model: &str) -> Value {
    let choice = resp.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
    let mut content = vec![];
    let mut stop = "end_turn";
    if let Some(ch) = choice {
        if let Some(t) = ch
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            if !t.is_empty() {
                content.push(json!({"type": "text", "text": t}));
            }
        }
        if let Some(calls) = ch
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .and_then(|c| c.as_array())
        {
            for call in calls {
                let f = &call["function"];
                let args: Value = f
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(json!({}));
                content.push(json!({
                    "type": "tool_use",
                    "id": s(&call["id"]),
                    "name": s(&f["name"]),
                    "input": args,
                }));
            }
        }
        if let Some(f) = ch.get("finish_reason").and_then(|f| f.as_str()) {
            stop = openai_finish_to_anthropic(f);
        }
    }
    let u = resp.get("usage");
    json!({
        "id": format!("msg_{}", s(&resp["id"]).trim_start_matches("chatcmpl-")),
        "type": "message",
        "role": "assistant",
        "model": echo_model,
        "content": content,
        "stop_reason": stop,
        "stop_sequence": null,
        "usage": {
            "input_tokens": u.and_then(|x| x.get("prompt_tokens")).and_then(|x| x.as_i64()).unwrap_or(0),
            "output_tokens": u.and_then(|x| x.get("completion_tokens")).and_then(|x| x.as_i64()).unwrap_or(0),
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": u.and_then(|x| x.get("prompt_tokens_details"))
                .and_then(|x| x.get("cached_tokens")).and_then(|x| x.as_i64()).unwrap_or(0),
        }
    })
}

// ---------------------------------------------------------------- 流式 ---

/// OpenAI SSE chunk → 0..n 條 Anthropic SSE data 行（不含 `data:` 前綴與 [DONE]）。
/// state 跨 chunk 累積工具調用參數（按 index）。
#[derive(Debug, Default)]
pub(crate) struct AnthropicStreamState {
    started: bool,
    text_open: bool,
    tool_blocks: std::collections::HashMap<i64, ToolAcc>,
    tool_order: Vec<i64>,
    text_closed: bool,
}

#[derive(Debug, Default)]
struct ToolAcc {
    id: String,
    name: String,
    args: String,
    block_index: i64,
}

pub(super) fn data_line(event: &str, payload: &Value) -> String {
    format!(
        "event: {event}\ndata: {}\n\n",
        serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string())
    )
}

pub(crate) fn openai_chunk_to_anthropic(
    state: &mut AnthropicStreamState,
    chunk: &Value,
    echo_model: &str,
    usage: &mut SseUsage,
) -> Vec<String> {
    let mut out = vec![];
    // 首個含 role 的 chunk 觸發 message_start
    if !state.started {
        state.started = true;
        out.push(data_line(
            "message_start",
            &json!({"type": "message_start", "message": {
                "id": format!("msg_{}", s(&chunk["id"]).trim_start_matches("chatcmpl-")),
                "type": "message", "role": "assistant", "model": echo_model,
                "content": [], "stop_reason": null, "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0,
                    "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0},
            }}),
        ));
    }
    let Some(choice) = chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
    else {
        return out;
    };
    // usage 可能出現在任何 chunk（通常是結尾），先記帳
    if let Some(u) = chunk.get("usage") {
        read_openai_usage(u, usage);
    }
    let Some(delta) = choice.get("delta") else {
        return out;
    };
    // 文本增量
    if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
        if !t.is_empty() {
            if !state.text_open {
                state.text_open = true;
                out.push(data_line(
                    "content_block_start",
                    &json!({"type": "content_block_start", "index": 0,
                        "content_block": {"type": "text", "text": ""}}),
                ));
            }
            out.push(data_line(
                "content_block_delta",
                &json!({"type": "content_block_delta", "index": 0,
                    "delta": {"type": "text_delta", "text": t}}),
            ));
        }
    }
    // 工具增量（按 index 累積參數，結束時一次性發 tool_use 塊）
    if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let idx = call.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
            let acc = state.tool_blocks.entry(idx).or_insert_with(|| {
                let bi = state.tool_order.len() as i64 + 100;
                state.tool_order.push(idx);
                ToolAcc {
                    block_index: bi,
                    ..Default::default()
                }
            });
            if let Some(id) = call.get("id").and_then(|i| i.as_str()) {
                if !id.is_empty() {
                    acc.id = id.to_string();
                }
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
            {
                if !name.is_empty() {
                    acc.name = name.to_string();
                }
            }
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
            {
                acc.args.push_str(args);
            }
        }
    }
    // 結束：關文本塊 → 發 tool_use 塊 → message_delta + message_stop
    if let Some(finish) = chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
    {
        if state.text_open && !state.text_closed {
            state.text_closed = true;
            out.push(data_line(
                "content_block_stop",
                &json!({"type": "content_block_stop", "index": 0}),
            ));
        }
        for idx in state.tool_order.clone() {
            if let Some(acc) = state.tool_blocks.get(&idx) {
                out.push(data_line(
                    "content_block_start",
                    &json!({"type": "content_block_start", "index": acc.block_index,
                        "content_block": {"type": "tool_use", "id": acc.id,
                            "name": acc.name, "input": {}}}),
                ));
                out.push(data_line(
                    "content_block_delta",
                    &json!({"type": "content_block_delta", "index": acc.block_index,
                        "delta": {"type": "input_json_delta", "partial_json": acc.args}}),
                ));
                out.push(data_line(
                    "content_block_stop",
                    &json!({"type": "content_block_stop", "index": acc.block_index}),
                ));
            }
        }
        let stop = openai_finish_to_anthropic(finish);
        out.push(data_line(
            "message_delta",
            &json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null},
                "usage": {"output_tokens": usage.out_tok}}),
        ));
        out.push(data_line("message_stop", &json!({"type": "message_stop"})));
    }
    out
}

/// Anthropic 串流錯誤事件行（上游 error → Claude Code 可見）。
pub(crate) fn anthropic_error_line(error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("upstream stream error");
    data_line(
        "error",
        &json!({"type": "error", "error": {"type": "api_error", "message": message}}),
    )
}
