//! `sanitize_chat_messages` 的消息配對修復測試。

use super::super::*;
use serde_json::Value;

fn chat_msgs(s: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(s)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn sanitize_repairs_orphan_tool_calls() {
    // 舊 session 續跑：c2 的 output 缺失（中斷輪次），補佔位 tool 消息
    let msgs = chat_msgs(
        r#"[
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "bash", "arguments": "{}"}},
                {"id": "c2", "type": "function", "function": {"name": "bash", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "done"},
            {"role": "user", "content": "next"}
        ]"#,
    );
    let out = sanitize_chat_messages(msgs);
    assert_eq!(out.len(), 5);
    assert_eq!(out[2]["role"], "tool");
    assert_eq!(out[2]["tool_call_id"], "c1");
    assert_eq!(out[3]["role"], "tool");
    assert_eq!(out[3]["tool_call_id"], "c2");
    assert!(
        out[3]["content"].as_str().unwrap().contains("unavailable"),
        "佔位內容可辨識"
    );
    assert_eq!(out[4]["content"], "next");
}

#[test]
fn sanitize_converts_orphan_tool_to_user() {
    // 孤兒 tool 消息（無對應 tool_calls）轉 user，保留內容
    let msgs = chat_msgs(
        r#"[
            {"role": "user", "content": "hi"},
            {"role": "tool", "tool_call_id": "zx", "content": "orphan-out"}
        ]"#,
    );
    let out = sanitize_chat_messages(msgs);
    assert_eq!(out.len(), 2);
    assert_eq!(out[1]["role"], "user");
    assert!(out[1]["content"].as_str().unwrap().contains("orphan-out"));
}

#[test]
fn sanitize_leaves_valid_history_untouched() {
    let msgs = chat_msgs(
        r#"[
            {"role": "system", "content": "s"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "bash", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "done"},
            {"role": "assistant", "content": "all good"}
        ]"#,
    );
    let out = sanitize_chat_messages(msgs.clone());
    assert_eq!(out, msgs, "有效歷史必須逐字不動");
}

#[test]
fn sanitize_removes_empty_tool_calls() {
    let msgs = chat_msgs(
        r#"[{"role": "assistant", "content": "hi", "tool_calls": []}]"#,
    );
    let out = sanitize_chat_messages(msgs);
    assert_eq!(out.len(), 1);
    assert!(out[0].get("tool_calls").is_none());
    assert_eq!(out[0]["content"], "hi");
}

#[test]
fn sanitize_generates_missing_ids() {
    // 空 id 的 call 補生成並同步改寫，佔位用同一 id
    let msgs = chat_msgs(
        r#"[{"role": "assistant", "content": null, "tool_calls": [
                {"type": "function", "function": {"name": "bash", "arguments": "{}"}}]}]"#,
    );
    let out = sanitize_chat_messages(msgs);
    assert_eq!(out.len(), 2);
    let gen = out[0]["tool_calls"][0]["id"].as_str().unwrap();
    assert!(!gen.is_empty());
    assert_eq!(out[1]["role"], "tool");
    assert_eq!(out[1]["tool_call_id"], gen);
}

#[test]
fn sanitize_merges_split_parallel_calls() {
    // 分裂的並行調用（逐條 assistant）→ 合併為一條，不補佔位
    let msgs = chat_msgs(
        r#"[
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c0", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "f", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c0", "content": "r0"},
            {"role": "tool", "tool_call_id": "c1", "content": "r1"}
        ]"#,
    );
    let out = sanitize_chat_messages(msgs);
    assert_eq!(out.len(), 4, "合併後應為 user+assistant+tool+tool");
    assert_eq!(out[1]["role"], "assistant");
    assert_eq!(out[1]["tool_calls"][0]["id"], "c0");
    assert_eq!(out[1]["tool_calls"][1]["id"], "c1");
    assert_eq!(out[2]["tool_call_id"], "c0");
    assert_eq!(out[3]["tool_call_id"], "c1");
}

#[test]
fn sanitize_fills_empty_non_assistant_content() {
    // 嚴格上游拒收空內容的非 assistant 消息（2026-09-24 codex 實例）
    let msgs: Vec<Value> = serde_json::from_str(
        r#"[
                {"role": "assistant", "tool_calls": [{"id": "k1", "type": "function",
                    "function": {"name": "f", "arguments": "{}"}}], "content": null},
                {"role": "tool", "tool_call_id": "k1", "content": ""},
                {"role": "user", "content": ""},
                {"role": "system", "content": []},
                {"role": "user", "content": "   "},
                {"role": "user", "content": [{"type": "input_text", "text": ""}]},
                {"role": "assistant", "content": ""},
                {"role": "user", "content": "hi"}
            ]"#,
    )
    .unwrap();
    let fixed = sanitize_chat_messages(msgs);
    // tool 空輸出 → MISSING_TOOL_OUTPUT（配對保留）
    assert_eq!(fixed[1]["tool_call_id"], "k1");
    assert!(!fixed[1]["content"].as_str().unwrap_or("").is_empty());
    // user/system 空 → 通用佔位
    assert_eq!(fixed[2]["content"], EMPTY_MESSAGE_PLACEHOLDER);
    assert_eq!(fixed[3]["content"], EMPTY_MESSAGE_PLACEHOLDER);
    assert_eq!(fixed[4]["content"], EMPTY_MESSAGE_PLACEHOLDER);
    assert_eq!(fixed[5]["content"], EMPTY_MESSAGE_PLACEHOLDER);
    // assistant 空文本不動（OpenAI 合法）
    assert_eq!(fixed[6]["content"], "");
    // 非空不動
    assert_eq!(fixed[7]["content"], "hi");
}
