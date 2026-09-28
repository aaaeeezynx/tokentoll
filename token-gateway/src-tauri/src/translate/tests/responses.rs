//! Responses ⇄ Chat Completions 的請求／回應映射、自訂工具與 reasoning 橋接測試。

use super::super::*;
use serde_json::Value;

#[test]
fn responses_request_mapping() {
    let req: Value = serde_json::from_str(
        r#"{
            "model": "glm-test",
            "instructions": "be nice",
            "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "message", "role": "assistant",
                 "content": [{"type": "output_text", "text": "ok"}]},
                {"type": "function_call", "call_id": "c1", "name": "bash", "arguments": "{\"cmd\":\"ls\"}"},
                {"type": "function_call_output", "call_id": "c1", "output": "done"},
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "hmm"}]}
            ],
            "tools": [{"type": "function", "name": "bash", "description": "run",
                       "parameters": {"type": "object"}},
                      {"type": "web_search", "search_context_size": "low"}],
            "tool_choice": "auto",
            "max_output_tokens": 64,
            "temperature": 0.5,
            "stream": true,
            "store": false
        }"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "actual-glm");
    assert_eq!(o["model"], "actual-glm");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "be nice");
    assert_eq!(msgs[1]["content"], "hi");
    assert_eq!(msgs[2]["content"], "ok");
    assert_eq!(msgs[3]["tool_calls"][0]["id"], "c1");
    assert_eq!(msgs[3]["tool_calls"][0]["function"]["name"], "bash");
    assert_eq!(msgs[4]["role"], "tool");
    assert_eq!(msgs[4]["tool_call_id"], "c1");
    assert_eq!(msgs.len(), 5, "reasoning 丟棄");
    assert_eq!(o["max_tokens"], 64);
    assert!(o.get("max_output_tokens").is_none());
    assert_eq!(o["temperature"], 0.5);
    assert_eq!(o["stream"], true);
    assert!(o.get("store").is_none());
    let tools = o["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1, "非 function tools 丟棄");
    assert_eq!(tools[0]["function"]["name"], "bash");
    assert_eq!(o["tool_choice"], "auto");
}

#[test]
fn responses_input_string_and_choice_object() {
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": "plain", "max_output_tokens": 8,
                "tool_choice": {"type": "function", "name": "bash"}}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    assert_eq!(o["messages"][0]["role"], "user");
    assert_eq!(o["messages"][0]["content"], "plain");
    assert_eq!(o["max_tokens"], 8);
    assert_eq!(o["tool_choice"]["function"]["name"], "bash");
}

#[test]
fn chat_response_to_responses_mapping() {
    let r: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-abc", "created": 1700000000, "model": "actual-glm",
            "choices": [{"message": {"role": "assistant", "content": "hi",
                "reasoning_content": "think"},
                "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                "prompt_tokens_details": {"cached_tokens": 3}}}"#,
    )
    .unwrap();
    let a = openai_to_responses(&r, "glm-test", &[]);
    assert_eq!(a["object"], "response");
    assert_eq!(a["status"], "completed");
    assert_eq!(a["model"], "glm-test");
    assert_eq!(a["output"][0]["type"], "reasoning");
    assert_eq!(a["output"][1]["type"], "message");
    assert_eq!(a["output"][1]["content"][0]["text"], "hi");
    assert_eq!(a["usage"]["input_tokens"], 10);
    assert_eq!(a["usage"]["output_tokens"], 5);
    assert_eq!(a["usage"]["total_tokens"], 15);
    assert_eq!(a["usage"]["input_tokens_details"]["cached_tokens"], 3);
    assert_eq!(a["created_at"], 1700000000);
}

#[test]
fn chat_response_incomplete_and_tool_calls() {
    let r: Value = serde_json::from_str(
        r#"{"id": "chatcmpl-x",
            "choices": [{"message": {"role": "assistant", "content": null,
                "tool_calls": [{"id": "c1", "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}}]},
                "finish_reason": "length"}],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2}}"#,
    )
    .unwrap();
    let a = openai_to_responses(&r, "m", &[]);
    assert_eq!(a["status"], "incomplete");
    assert_eq!(a["incomplete_details"]["reason"], "max_output_tokens");
    assert_eq!(a["output"][0]["type"], "function_call");
    assert_eq!(a["output"][0]["call_id"], "c1");
    assert_eq!(a["output"][0]["arguments"], "{\"cmd\":\"ls\"}");
}

#[test]
fn responses_parallel_calls_merged() {
    // 端到端：連發 3 個 function_call + 3 個 output → 一條 assistant 帶 3 個 tool_calls
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "function_call", "call_id": "k0", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k2", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "k0", "output": "r0"},
                {"type": "function_call_output", "call_id": "k1", "output": "r1"},
                {"type": "function_call_output", "call_id": "k2", "output": "r2"}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 5, "應為 user+assistant+tool*3");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["tool_calls"].as_array().unwrap().len(), 3);
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "k0");
    assert_eq!(msgs[1]["tool_calls"][2]["id"], "k2");
    assert_eq!(msgs[2]["tool_call_id"], "k0");
    assert_eq!(msgs[4]["tool_call_id"], "k2");
}

#[test]
fn responses_interleaved_calls_stay_paired() {
    // 交錯 call/output 不合併，但各自配對
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "function_call", "call_id": "a", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "a", "output": "ra"},
                {"type": "function_call", "call_id": "b", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "b", "output": "rb"}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0]["tool_calls"][0]["id"], "a");
    assert_eq!(msgs[1]["tool_call_id"], "a");
    assert_eq!(msgs[2]["tool_calls"][0]["id"], "b");
    assert_eq!(msgs[3]["tool_call_id"], "b");
}

#[test]
fn responses_orphan_function_call_gets_placeholder() {
    // 端到端：responses input 只有 function_call 沒有 output（舊中斷 session）
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "function_call", "call_id": "c9", "name": "bash", "arguments": "{}"}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "c9");
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["tool_call_id"], "c9");
}

// ------------------------------------------------ 自訂（freeform）工具 ---

fn custom_req() -> Value {
    serde_json::from_str(
        r#"{"model": "m",
            "tools": [
                {"type": "function", "name": "exec_command", "description": "run",
                 "parameters": {"type": "object"}},
                {"type": "custom", "name": "apply_patch", "description": "patch files",
                 "format": {"type": "grammar", "syntax": "lark", "definition": "start: x"}}
            ],
            "input": [
                {"type": "message", "role": "user", "content": "fix"},
                {"type": "custom_tool_call", "call_id": "ctc1", "name": "apply_patch",
                 "input": "*** Begin Patch\n*** End Patch"},
                {"type": "custom_tool_call_output", "call_id": "ctc1",
                 "output": "Success. Updated the following files:\nM a.ts"}
            ]}"#,
    )
    .unwrap()
}

#[test]
fn responses_custom_tool_names_extracted() {
    assert_eq!(custom_tool_names(&custom_req()), vec!["apply_patch".to_string()]);
}

#[test]
fn responses_custom_tool_functionalized() {
    let o = responses_to_openai(&custom_req(), "m");
    let tools = o["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2, "function + custom 都要送出");
    let ap = tools
        .iter()
        .find(|t| t["function"]["name"] == "apply_patch")
        .unwrap();
    assert_eq!(ap["type"], "function");
    assert_eq!(ap["function"]["parameters"]["required"][0], "input");
    assert!(ap["function"]["description"]
        .as_str()
        .unwrap()
        .contains("input"));
    // 歷史：custom_tool_call → assistant tool_calls（arguments 包成 {"input": ...}）
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "ctc1");
    assert_eq!(msgs[1]["tool_calls"][0]["function"]["name"], "apply_patch");
    let args: Value =
        serde_json::from_str(msgs[1]["tool_calls"][0]["function"]["arguments"].as_str().unwrap())
            .unwrap();
    assert!(args["input"].as_str().unwrap().contains("Begin Patch"));
    // 歷史：custom_tool_call_output → tool 消息
    assert_eq!(msgs[2]["role"], "tool");
    assert_eq!(msgs[2]["tool_call_id"], "ctc1");
    assert!(msgs[2]["content"].as_str().unwrap().contains("Success"));
}

#[test]
fn responses_to_openai_empty_tool_output_filled() {
    // Codex 空 function_call_output → 翻譯後 tool content 不為空
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "k1", "output": {"output": ""}},
                {"type": "custom_tool_call", "call_id": "k2", "name": "c", "input": "x"},
                {"type": "custom_tool_call_output", "call_id": "k2", "output": []}
            ]}"#,
    )
    .unwrap();
    let out = responses_to_openai(&req, "m");
    let msgs = out["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert!(!msgs[1]["content"].as_str().unwrap_or("").is_empty());
    assert!(!msgs[3]["content"].as_str().unwrap_or("").is_empty());
}

#[test]
fn reasoning_effort_mapping_filters_unsupported() {
    let req: Value =
        serde_json::from_str(r#"{"model": "m", "reasoning": {"effort": "medium"}}"#).unwrap();
    assert_eq!(responses_to_openai(&req, "m")["reasoning_effort"], "medium");
    let req: Value =
        serde_json::from_str(r#"{"model": "m", "reasoning": {"effort": "low"}}"#).unwrap();
    assert!(responses_to_openai(&req, "m").get("reasoning_effort").is_none());
    let req: Value = serde_json::from_str(r#"{"model": "m"}"#).unwrap();
    assert!(responses_to_openai(&req, "m").get("reasoning_effort").is_none());
}

// ------------------------------------------------- reasoning 橋接 ---

#[test]
fn reasoning_bridge_restores_reasoning_content() {
    // Codex 存回的 reasoning item（summary 帶思考）→ 後續 assistant 訊息帶 reasoning_content
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "reasoning", "id": "r1", "summary": [{"type": "summary_text", "text": "plan: run ls"}]},
                {"type": "function_call", "call_id": "c1", "name": "bash", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": "ok"},
                {"type": "reasoning", "id": "r2", "summary": [{"type": "summary_text", "text": "next: edit file"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["reasoning_content"], "plan: run ls");
    assert_eq!(msgs[1]["tool_calls"][0]["id"], "c1");
    assert_eq!(msgs[3]["role"], "assistant");
    assert_eq!(msgs[3]["reasoning_content"], "next: edit file");
    assert_eq!(msgs[3]["content"], "done");
}

#[test]
fn reasoning_bridge_does_not_leak_across_user_turn() {
    // reasoning 後面沒有 assistant（或先遇到 user）→ 不應掛到後面的訊息
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "reasoning", "id": "r1", "summary": [{"type": "summary_text", "text": "stale"}]},
                {"type": "message", "role": "user", "content": "next"},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "ok"}]}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    let msgs = o["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "user");
    assert!(msgs[1].get("reasoning_content").is_none());
}

#[test]
fn reasoning_bridge_reads_marker_from_encrypted_content() {
    let req: Value = serde_json::from_str(
        r#"{"model": "m", "input": [
                {"type": "reasoning", "id": "r1", "summary": [],
                 "encrypted_content": "tg-openai-reasoning-v1:reasoning:raw think"},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "x"}]}
            ]}"#,
    )
    .unwrap();
    let o = responses_to_openai(&req, "m");
    assert_eq!(o["messages"][0]["reasoning_content"], "raw think");
}

#[test]
fn prompt_cache_key_passthrough() {
    let req: Value =
        serde_json::from_str(r#"{"model": "m", "prompt_cache_key": "sess-123"}"#).unwrap();
    assert_eq!(responses_to_openai(&req, "m")["prompt_cache_key"], "sess-123");
    let req: Value = serde_json::from_str(r#"{"model": "m"}"#).unwrap();
    assert!(responses_to_openai(&req, "m").get("prompt_cache_key").is_none());
}
