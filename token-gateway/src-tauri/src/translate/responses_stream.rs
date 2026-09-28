//! Responses 流式：跨 chunk 累積的 `ResponsesStreamState` 與
//! OpenAI Chat chunk → Responses SSE 事件（`chat_chunk_to_responses`）。

use super::*;
use serde_json::{json, Value};

use super::anthropic::data_line;
use super::responses_tools::{is_custom_tool, tool_item, RToolAcc};

pub(super) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn responses_skeleton(id: &str, model: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "created_at": now_secs(),
        "status": "in_progress",
        "model": model,
        "output": [],
    })
}

/// OpenAI SSE chunk → Responses SSE 事件行（含 `event:` 前綴）。
/// state 跨 chunk 累積文本與工具調用；`response.completed` 延後到流結束
/// 由 `responses_stream_finish` 補發（usage 常在 finish chunk 之後的獨立 chunk）。
#[derive(Debug, Default)]
pub(crate) struct ResponsesStreamState {
    pub(super) created: bool,
    pub(super) completed: bool,
    pub(super) failed: bool,
    pub(super) saw_finish: bool,
    pub(super) saw_done: bool,
    pub(super) text_open: bool,
    reasoning_open: bool,
    pub(super) msg_id: String,
    pub(super) resp_id: String,
    pub(super) full_text: String,
    pub(super) reasoning_text: String,
    pub(super) tools: std::collections::HashMap<i64, RToolAcc>,
    pub(super) tool_order: Vec<i64>,
    next_idx: i64,
}

impl ResponsesStreamState {
    /// 上游送出 `[DONE]` 時標記：有它即視為正常收尾（部分上游不發 finish_reason）。
    pub(crate) fn note_done(&mut self) {
        self.saw_done = true;
    }

    pub(super) fn reasoning_id(&self) -> String {
        format!("{}_rs", self.msg_id)
    }

    /// reasoning 收尾：summary_text.done → part.done → output_item.done。
    /// 思考以 summary 事件送出（Codex 只認 reasoning_summary_*，不認 reasoning_text_*）。
    pub(super) fn close_reasoning(&mut self) -> Vec<String> {
        if !self.reasoning_open {
            return vec![];
        }
        self.reasoning_open = false;
        let id = self.reasoning_id();
        let full = self.reasoning_text.clone();
        vec![
            data_line(
                "response.reasoning_summary_text.done",
                &json!({
                    "type": "response.reasoning_summary_text.done",
                    "item_id": id, "output_index": 0, "summary_index": 0,
                    "text": full,
                }),
            ),
            data_line(
                "response.reasoning_summary_part.done",
                &json!({
                    "type": "response.reasoning_summary_part.done",
                    "item_id": id, "output_index": 0, "summary_index": 0,
                    "part": {"type": "summary_text", "text": full},
                }),
            ),
            data_line(
                "response.output_item.done",
                &json!({
                    "type": "response.output_item.done", "output_index": 0,
                    "item": {"type": "reasoning", "id": id,
                        "summary": [{"type": "summary_text", "text": full}],
                        "content": null},
                }),
            ),
        ]
    }
}

pub(crate) fn chat_chunk_to_responses(
    state: &mut ResponsesStreamState,
    chunk: &Value,
    echo_model: &str,
    usage: &mut SseUsage,
    custom_tools: &[String],
) -> Vec<String> {
    let mut out = vec![];
    // usage 可能出現在任何 chunk（通常在 finish 之後的獨立 chunk），先記帳
    if let Some(u) = chunk.get("usage") {
        read_openai_usage(u, usage);
    }
    if !state.created {
        state.created = true;
        let cid = s(&chunk["id"]).trim_start_matches("chatcmpl-").to_string();
        state.resp_id = if cid.is_empty() {
            format!("resp_{}", now_secs())
        } else {
            format!("resp_{cid}")
        };
        out.push(data_line(
            "response.created",
            &json!({
                "type": "response.created",
                "response": responses_skeleton(&state.resp_id, echo_model),
            }),
        ));
    }
    if state.msg_id.is_empty() {
        let cid = s(&chunk["id"]).trim_start_matches("chatcmpl-").to_string();
        state.msg_id = if cid.is_empty() {
            format!("msg_{}", now_secs())
        } else {
            format!("msg_{cid}")
        };
    }
    let Some(delta) = chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("delta"))
    else {
        return out;
    };
    // reasoning_content → reasoning item + reasoning_summary_text.delta
    //（Codex 只認 summary 事件；thinking 會被存進 reasoning item 的 summary，
    //  下一輪由 responses_to_openai 還原成 assistant 的 reasoning_content）
    if let Some(rt) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
        if !rt.is_empty() {
            if !state.reasoning_open {
                state.reasoning_open = true;
                out.push(data_line(
                    "response.output_item.added",
                    &json!({
                        "type": "response.output_item.added", "output_index": 0,
                        "item": {"type": "reasoning", "id": state.reasoning_id(),
                            "summary": [], "content": null},
                    }),
                ));
                out.push(data_line(
                    "response.reasoning_summary_part.added",
                    &json!({
                        "type": "response.reasoning_summary_part.added",
                        "item_id": state.reasoning_id(), "output_index": 0,
                        "summary_index": 0,
                        "part": {"type": "summary_text", "text": ""},
                    }),
                ));
            }
            state.reasoning_text.push_str(rt);
            out.push(data_line(
                "response.reasoning_summary_text.delta",
                &json!({
                    "type": "response.reasoning_summary_text.delta",
                    "item_id": state.reasoning_id(),
                    "output_index": 0, "summary_index": 0, "delta": rt,
                }),
            ));
        }
    }
    // 文本增量
    if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
        if !t.is_empty() {
            if !state.text_open {
                state.text_open = true;
                out.push(data_line(
                    "response.output_item.added",
                    &json!({
                        "type": "response.output_item.added", "output_index": 0,
                        "item": {"type": "message", "id": state.msg_id, "role": "assistant",
                            "status": "in_progress", "content": []},
                    }),
                ));
            }
            state.full_text.push_str(t);
            out.push(data_line(
                "response.output_text.delta",
                &json!({
                    "type": "response.output_text.delta", "item_id": state.msg_id,
                    "output_index": 0, "content_index": 0, "delta": t,
                }),
            ));
        }
    }
    // 工具增量累積（按 index）
    if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let idx = call.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
            let acc = state.tools.entry(idx).or_default();
            if !state.tool_order.contains(&idx) {
                state.tool_order.push(idx);
            }
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
    // 結束：關 reasoning → 關文本 → 發 output_item.done（消息 + 工具）；completed 延後
    if chunk
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .is_some()
    {
        state.saw_finish = true;
        if state.reasoning_open {
            out.extend(state.close_reasoning());
        }
        if state.text_open {
            state.text_open = false;
            let full = state.full_text.clone();
            out.push(data_line(
                "response.output_item.done",
                &json!({
                    "type": "response.output_item.done", "output_index": 0,
                    "item": {"type": "message", "id": state.msg_id, "role": "assistant",
                        "status": "completed",
                        "content": [{"type": "output_text", "text": full, "annotations": []}]},
                }),
            ));
        }
        for idx in state.tool_order.clone() {
            if let Some(acc) = state.tools.get(&idx) {
                let oi = state.next_idx;
                state.next_idx += 1;
                let cid = if acc.id.is_empty() {
                    format!("call_{idx}")
                } else {
                    acc.id.clone()
                };
                out.push(data_line(
                    "response.output_item.done",
                    &json!({
                        "type": "response.output_item.done", "output_index": oi,
                        "item": tool_item(acc, &cid, is_custom_tool(custom_tools, &acc.name)),
                    }),
                ));
            }
        }
    }
    out
}
