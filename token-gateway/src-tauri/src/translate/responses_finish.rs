//! Responses 流式的收尾事件：上游錯誤／提前中斷的 `response.failed`，
//! 以及流結束後的補發（關未完成 item + `response.completed`）。

use super::*;
use serde_json::{json, Value};

use super::anthropic::data_line;
use super::responses_stream::{now_secs, responses_skeleton, ResponsesStreamState};
use super::responses_tools::{is_custom_tool, tool_item};

/// 上游串流錯誤 / 提前中斷 → `response.failed`（不再靜默當成正常完成）。
/// 回傳空 vec 表示先前已發過 failed。
pub(crate) fn responses_stream_fail(
    state: &mut ResponsesStreamState,
    echo_model: &str,
    error: &Value,
) -> Vec<String> {
    if state.failed {
        return vec![];
    }
    state.failed = true;
    state.completed = true;
    let mut out = vec![];
    if !state.created {
        state.created = true;
        let id = if state.resp_id.is_empty() {
            format!("resp_{}", now_secs())
        } else {
            state.resp_id.clone()
        };
        out.push(data_line(
            "response.created",
            &json!({
                "type": "response.created",
                "response": responses_skeleton(&id, echo_model),
            }),
        ));
    }
    let id = if state.resp_id.is_empty() {
        format!("resp_{}", now_secs())
    } else {
        state.resp_id.clone()
    };
    let message = error
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("上游串流錯誤");
    let code = error
        .get("code")
        .and_then(|c| c.as_str())
        .or_else(|| error.get("type").and_then(|t| t.as_str()))
        .unwrap_or("upstream_error");
    out.push(data_line(
        "response.failed",
        &json!({
            "type": "response.failed",
            "response": {
                "id": id,
                "object": "response",
                "created_at": now_secs(),
                "status": "failed",
                "model": echo_model,
                "output": [],
                "error": {"code": code, "message": message},
            },
        }),
    ));
    out
}

/// 流結束後補發：異常中斷先關未完成的 item，再發 `response.completed`（含 usage）；
/// 未收到 finish_reason 且未見 `[DONE]` 的提前中斷改發 `response.failed`。
pub(crate) fn responses_stream_finish(
    state: &mut ResponsesStreamState,
    echo_model: &str,
    usage: &SseUsage,
    custom_tools: &[String],
) -> Vec<String> {
    if state.failed {
        return vec![];
    }
    if !state.saw_finish && !state.saw_done {
        return responses_stream_fail(
            state,
            echo_model,
            &json!({
                "code": "upstream_stream_interrupted",
                "message": "上游串流在完成前中斷（未收到 finish_reason / [DONE]）",
            }),
        );
    }
    let mut out = vec![];
    out.extend(state.close_reasoning());
    if state.text_open {
        state.text_open = false;
        if !state.full_text.is_empty() {
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
    }
    if !state.completed {
        state.completed = true;
        let mut output: Vec<Value> = vec![];
        if !state.reasoning_text.is_empty() {
            output.push(json!({
                "type": "reasoning", "id": state.reasoning_id(),
                "summary": [{"type": "summary_text", "text": state.reasoning_text.clone()}],
                "content": null,
            }));
        }
        if !state.full_text.is_empty() {
            output.push(json!({
                "type": "message", "id": state.msg_id, "role": "assistant",
                "status": "completed",
                "content": [{"type": "output_text", "text": state.full_text.clone(), "annotations": []}],
            }));
        }
        for idx in state.tool_order.clone() {
            if let Some(acc) = state.tools.get(&idx) {
                let cid = if acc.id.is_empty() {
                    format!("call_{idx}")
                } else {
                    acc.id.clone()
                };
                output.push(tool_item(acc, &cid, is_custom_tool(custom_tools, &acc.name)));
            }
        }
        let (inp, outp) = (usage.in_tok, usage.out_tok);
        out.push(data_line(
            "response.completed",
            &json!({
                "type": "response.completed",
                "response": {
                    "id": state.resp_id,
                    "object": "response",
                    "created_at": now_secs(),
                    "status": "completed",
                    "model": echo_model,
                    "output": output,
                    "usage": {
                        "input_tokens": inp,
                        "input_tokens_details": {"cached_tokens": usage.cache_read},
                        "output_tokens": outp,
                        "output_tokens_details": {"reasoning_tokens": 0},
                        "total_tokens": inp + outp,
                    },
                },
            }),
        ));
    }
    out
}
