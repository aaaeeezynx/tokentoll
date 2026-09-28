//! Responses 流式事件序列（`chat_chunk_to_responses` / `responses_stream_finish` /
//! `responses_stream_fail`）的測試。

use super::super::*;
use serde_json::{json, Value};

#[test]
fn responses_stream_sequence_shapes() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-x", "choices": [{"delta": {"role": "assistant", "content": "he"}}]}"#,
    )
    .unwrap();
    let lines = chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]);
    let joined = lines.join("");
    assert!(joined.contains("event: response.created"));
    assert!(joined.contains("event: response.output_item.added"));
    assert!(joined.contains("response.output_text.delta"));
    assert!(joined.contains("\"delta\":\"he\""));
    let c2: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-x", "choices": [{"delta": {"content": "llo"}, "finish_reason": "stop"}]}"#,
    )
    .unwrap();
    let lines = chat_chunk_to_responses(&mut st, &c2, "glm", &mut usage, &[]);
    let joined = lines.join("");
    assert!(joined.contains("event: response.output_item.done"));
    assert!(joined.contains("\"text\":\"hello\""));
    // usage 在 finish 之後的獨立 chunk
    let c3: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-x", "choices": [], "usage": {"prompt_tokens": 7, "completion_tokens": 3}}"#,
    )
    .unwrap();
    let lines = chat_chunk_to_responses(&mut st, &c3, "glm", &mut usage, &[]);
    assert!(!lines.join("").contains("response.completed"));
    assert_eq!((usage.in_tok, usage.out_tok), (7, 3));
    // 流結束補發 completed
    let finals = responses_stream_finish(&mut st, "glm", &usage, &[]);
    let joined = finals.join("");
    assert!(joined.contains("event: response.completed"));
    assert!(joined.contains("\"input_tokens\":7"));
    assert!(joined.contains("\"output_tokens\":3"));
    // 重複調用不重發
    assert!(responses_stream_finish(&mut st, "glm", &usage, &[]).is_empty());
}

#[test]
fn responses_stream_reasoning_and_tools() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-y", "choices": [{"delta": {"role": "assistant", "reasoning_content": "think"}}]}"#,
    )
    .unwrap();
    let joined = chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]).join("");
    assert!(joined.contains("response.reasoning_summary_text.delta"));
    assert!(!joined.contains("response.reasoning_text.delta"));
    let c2: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-y", "choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": "c1", "function": {"name": "bash", "arguments": "{\"cmd\":"}}]}}]}"#,
    )
    .unwrap();
    chat_chunk_to_responses(&mut st, &c2, "glm", &mut usage, &[]);
    let c3: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-y", "choices": [{"delta": {"tool_calls": [
                {"index": 0, "function": {"arguments": "\"ls\"}"}}]}, "finish_reason": "tool_calls"}]}"#,
    )
    .unwrap();
    let joined = chat_chunk_to_responses(&mut st, &c3, "glm", &mut usage, &[]).join("");
    assert!(joined.contains("\"type\":\"function_call\""));
    assert!(joined.contains("\"arguments\":\"{\\\"cmd\\\":\\\"ls\\\"}\""));
    let finals = responses_stream_finish(&mut st, "glm", &usage, &[]).join("");
    assert!(finals.contains("event: response.completed"));
    assert!(finals.contains("\"type\":\"function_call\""));
}

#[test]
fn responses_stream_custom_tool_call_restored() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let customs = vec!["apply_patch".to_string()];
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-z", "choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": "call_9", "function": {"name": "apply_patch",
                 "arguments": "{\"input\":\"*** Begin Patch\\n*** End Patch\"}"}}]}}]}"#,
    )
    .unwrap();
    chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &customs);
    let c2: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-z", "choices": [{"delta": {}, "finish_reason": "tool_calls"}]}"#,
    )
    .unwrap();
    let joined = chat_chunk_to_responses(&mut st, &c2, "glm", &mut usage, &customs).join("");
    assert!(joined.contains("\"type\":\"custom_tool_call\""));
    assert!(joined.contains("\"name\":\"apply_patch\""));
    assert!(joined.contains("\"input\":\"*** Begin Patch\\n*** End Patch\""));
    let finals = responses_stream_finish(&mut st, "glm", &usage, &customs).join("");
    assert!(finals.contains("event: response.completed"));
    assert!(finals.contains("\"type\":\"custom_tool_call\""));
    assert!(!finals.contains("\"type\":\"function_call\""));
}

#[test]
fn responses_stream_interrupted_emits_failed() {        let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-i", "choices": [{"delta": {"content": "partial"}}]}"#,
    )
    .unwrap();
    chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]);
    // 無 finish_reason、無 [DONE] → 不能假裝 completed
    let finals = responses_stream_finish(&mut st, "glm", &usage, &[]).join("");
    assert!(finals.contains("event: response.failed"));
    assert!(finals.contains("upstream_stream_interrupted"));
    assert!(!finals.contains("response.completed"));
    // 已 failed 後不再補發
    assert!(responses_stream_finish(&mut st, "glm", &usage, &[]).is_empty());
    assert!(responses_stream_fail(&mut st, "glm", &json!({"message": "x"})).is_empty());
}

#[test]
fn responses_stream_done_without_finish_is_completed() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-d", "choices": [{"delta": {"content": "ok"}}]}"#,
    )
    .unwrap();
    chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]);
    st.note_done();
    let finals = responses_stream_finish(&mut st, "glm", &usage, &[]).join("");
    assert!(finals.contains("event: response.completed"));
    assert!(!finals.contains("response.failed"));
}

#[test]
fn responses_stream_fail_shape() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-f", "choices": [{"delta": {"content": "x"}}]}"#,
    )
    .unwrap();
    chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]);
    let lines = responses_stream_fail(
        &mut st,
        "glm",
        &json!({"message": "upstream boom", "type": "rate_limit_error"}),
    )
    .join("");
    assert!(lines.contains("event: response.failed"));
    assert!(lines.contains("\"status\":\"failed\""));
    assert!(lines.contains("upstream boom"));
    assert!(lines.contains("rate_limit_error"));
}

#[test]
fn stream_reasoning_emits_summary_events_and_item() {
    let mut st = ResponsesStreamState::default();
    let mut usage = SseUsage::default();
    let c1: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-rz", "choices": [{"delta": {"reasoning_content": "think A"}}]}"#,
    )
    .unwrap();
    let joined = chat_chunk_to_responses(&mut st, &c1, "glm", &mut usage, &[]).join("");
    assert!(joined.contains("event: response.reasoning_summary_part.added"));
    assert!(joined.contains("event: response.reasoning_summary_text.delta"));
    assert!(joined.contains("think A"));
    let c2: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-rz", "choices": [{"delta": {"content": "hi"}, "finish_reason": "stop"}]}"#,
    )
    .unwrap();
    let joined = chat_chunk_to_responses(&mut st, &c2, "glm", &mut usage, &[]).join("");
    assert!(joined.contains("event: response.reasoning_summary_text.done"));
    assert!(joined.contains("event: response.reasoning_summary_part.done"));
    assert!(joined.contains("\"type\":\"reasoning\""));
    assert!(joined.contains("\"summary\":[{\"text\":\"think A\",\"type\":\"summary_text\"}]")
        || joined.contains("\"summary\":[{\"type\":\"summary_text\",\"text\":\"think A\"}]"));
    let finals = responses_stream_finish(&mut st, "glm", &usage, &[]).join("");
    assert!(finals.contains("event: response.completed"));
    assert!(finals.contains("\"type\":\"reasoning\""));
    assert!(finals.contains("think A"));
}
