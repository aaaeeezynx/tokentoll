//! Anthropic Messages ⇄ OpenAI Chat 的非流式映射測試。

use super::super::*;
use serde_json::Value;

fn anth_req() -> Value {
    serde_json::from_str(
        r#"{
            "model": "claude-sonnet-4-6",
            "max_tokens": 100,
            "system": "你是助手",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [{"type": "text", "text": "ok"},
                    {"type": "tool_use", "id": "t1", "name": "bash", "input": {"cmd": "ls"}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "a"}]}
            ],
            "tools": [{"name": "bash", "description": "run", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "auto"},
            "stream": true
        }"#,
    )
    .unwrap()
}

#[test]
fn request_mapping() {
    let o = anthropic_to_openai(&anth_req(), "deepseek-v4-flash");
    assert_eq!(o["model"], "deepseek-v4-flash");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[1]["content"], "hi");
    assert_eq!(msgs[2]["tool_calls"][0]["function"]["name"], "bash");
    assert_eq!(msgs[3]["role"], "tool");
    assert_eq!(msgs[3]["tool_call_id"], "t1");
    assert_eq!(o["tools"][0]["function"]["parameters"]["type"], "object");
    assert_eq!(o["tool_choice"], "auto");
    assert_eq!(o["stream"], true);
    // max_tokens 直通，top_k 等無對等物不出現
    assert_eq!(o["max_tokens"], 100);
    assert!(o.get("top_k").is_none());
}

#[test]
fn response_mapping() {
    let r: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-abc", "model": "deepseek-v4-flash",
            "choices": [{"message": {"content": "hi", "tool_calls": [
                {"id": "c1", "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}}]},
                "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5,
                "prompt_tokens_details": {"cached_tokens": 3}}}"#,
    )
    .unwrap();
    let a = openai_to_anthropic(&r, "claude-sonnet-4-6");
    assert_eq!(a["type"], "message");
    assert_eq!(a["model"], "claude-sonnet-4-6");
    assert_eq!(a["content"][0]["type"], "text");
    assert_eq!(a["content"][1]["type"], "tool_use");
    assert_eq!(a["content"][1]["input"]["cmd"], "ls");
    assert_eq!(a["stop_reason"], "tool_use");
    assert_eq!(a["usage"]["input_tokens"], 10);
    assert_eq!(a["usage"]["output_tokens"], 5);
    assert_eq!(a["usage"]["cache_read_input_tokens"], 3);
    assert!(a["id"].as_str().unwrap().starts_with("msg_"));
}

#[test]
fn anthropic_orphan_tool_use_gets_placeholder() {
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [{"type": "tool_use", "id": "t9", "name": "bash", "input": {}}]}
            ]}"#,
    )
    .unwrap();
    let o = anthropic_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["tool_call_id"], "t9");
}
