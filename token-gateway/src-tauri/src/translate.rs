//! M7：Anthropic Messages ⇄ OpenAI Chat Completions 翻譯。
//!
//! 只做 **Anthropic 進 → OpenAI 出**（Claude Code 經網關打 OpenAI 兼容上游，
//! 即用戶現有的 cc-switch 流程）。反向遇到時回明確 400，不靜默轉發。
//!
//! 模型名映射走可用模型表（display_name → actual_model），見 `models::resolve_actual`。

use serde_json::{json, Value};

/// 流式/非流式共用的用量累積（代理記帳用）。
#[derive(Debug, Default, Clone)]
pub(crate) struct SseUsage {
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub model: Option<String>,
}

impl SseUsage {
    pub fn total(&self) -> i64 {
        self.in_tok + self.out_tok + self.cache_read + self.cache_write
    }
}

pub(crate) fn num(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key)?.as_i64()
}

/// OpenAI usage 對象 → 累積（chat / responses 通用）。
pub(crate) fn read_openai_usage(u: &serde_json::Value, acc: &mut SseUsage) {
    if let Some(n) = num(u, "prompt_tokens").or(num(u, "input_tokens")) {
        acc.in_tok = n;
    }
    if let Some(n) = num(u, "completion_tokens").or(num(u, "output_tokens")) {
        acc.out_tok = n;
    }
    if let Some(n) = num(u, "cache_read_input_tokens") {
        acc.cache_read = n;
    }
    if let Some(n) = num(u, "cache_creation_input_tokens") {
        acc.cache_write = n;
    }
    if let Some(det) = u.get("prompt_tokens_details") {
        if let Some(n) = num(det, "cached_tokens") {
            acc.cache_read = n;
        }
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// 缺失的歷史 tool 輸出佔位（舊 session 續跑 / 中斷輪次常見）：讓上游配對校驗通過，
/// 模型按現有上下文繼續，不會把佔位當真實執行結果。
const MISSING_TOOL_OUTPUT: &str =
    "[Previous tool result unavailable — restored from session history; continue with available context.]";

/// 合併連續的 assistant 消息：只有上一個帶 tool_calls 才吸收後續 assistant
///（文本在前、調用在後的形狀本就合法，不動，最小改動）。
/// tool_calls 拼接、文本內容按序拼接。把分裂的並行調用還原為
/// 一條 assistant + 後續 tool 消息的標準形狀，避免「其後非 tool」拒收。
fn merge_assistants(messages: Vec<Value>) -> Vec<Value> {
    fn has_calls(m: &Value) -> bool {
        m.get("tool_calls")
            .and_then(|c| c.as_array())
            .is_some_and(|a| !a.is_empty())
    }
    fn text_part(v: Value) -> Value {
        match v {
            Value::String(t) => json!({"type": "text", "text": t}),
            Value::Null => json!({"type": "text", "text": ""}),
            other => other,
        }
    }
    fn merge_content(a: Value, b: Value) -> Value {
        match (a, b) {
            (Value::Null, x) | (x, Value::Null) => x,
            (Value::String(mut s1), Value::String(s2)) => {
                if !s1.is_empty() && !s2.is_empty() {
                    s1.push('\n');
                }
                s1.push_str(&s2);
                Value::String(s1)
            }
            (Value::Array(mut p1), Value::Array(p2)) => {
                p1.extend(p2);
                Value::Array(p1)
            }
            (Value::Array(mut p1), other) => {
                p1.push(text_part(other));
                Value::Array(p1)
            }
            (other, Value::Array(mut p2)) => {
                let mut v = vec![text_part(other)];
                v.append(&mut p2);
                Value::Array(v)
            }
            (x, y) => Value::Array(vec![text_part(x), text_part(y)]),
        }
    }
    let mut out: Vec<Value> = Vec::with_capacity(messages.len());
    for m in messages {
        let is_asst = m.get("role").and_then(|r| r.as_str()) == Some("assistant");
        let prev = out.last();
        let prev_is_asst = prev.is_some_and(|l| {
            l.get("role").and_then(|r| r.as_str()) == Some("assistant")
        });
        let prev_has_calls = out.last().is_some_and(has_calls);
        // 只有上一個是帶 tool_calls 的 assistant 才合併（保護其後續鄰接）；
        // 文本 assistant 之間、文本在前調用在後的形狀本就合法，原樣保留
        if is_asst && prev_is_asst && prev_has_calls {
            let mut last = out.pop().unwrap();
            let mut calls: Vec<Value> = last
                .get("tool_calls")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            if let Some(more) = m.get("tool_calls").and_then(|c| c.as_array()) {
                calls.extend(more.clone());
            }
            if let Some(obj) = last.as_object_mut() {
                if calls.is_empty() {
                    obj.remove("tool_calls");
                } else {
                    obj.insert("tool_calls".to_string(), Value::Array(calls));
                }
                let merged = merge_content(
                    obj.remove("content").unwrap_or(Value::Null),
                    m.get("content").cloned().unwrap_or(Value::Null),
                );
                obj.insert("content".to_string(), merged);
            }
            out.push(last);
        } else {
            out.push(m);
        }
    }
    out
}

/// OpenAI 兼容上游的硬性配對規則：assistant.tool_calls 的每個 id，其後必須緊跟
/// role=tool 且 tool_call_id 對應的消息；孤兒 tool 消息（無對應 tool_calls）
/// 同樣會被嚴格上游拒收。本函數做最小修復：
/// 連續 assistant 先合併（還原並行調用形狀）；缺 output 補佔位 tool 消息；
/// 孤兒 tool 消息轉 user（保留內容）；空 tool_calls 數組移除；
/// 空 id 補生成並同步改寫。已配對的歷史原樣不動。
/// 最後非 assistant 空 content 補佔位（嚴格上游拒收空內容）。
pub(crate) fn sanitize_chat_messages(messages: Vec<Value>) -> Vec<Value> {
    use std::collections::HashSet;
    // 第零遍：合併分裂的並行調用
    let messages = merge_assistants(messages);
    // 全局聲明的 tool_call id 集合
    let mut declared: HashSet<String> = HashSet::new();
    for m in &messages {
        if m.get("role").and_then(|r| r.as_str()) == Some("assistant") {
            if let Some(calls) = m.get("tool_calls").and_then(|c| c.as_array()) {
                for c in calls {
                    let id = s(&c["id"]);
                    if !id.is_empty() {
                        declared.insert(id);
                    }
                }
            }
        }
    }
    // 第一遍：孤兒 tool 消息 → user（保留內容，避免嚴格上游拒收）
    let mut norm: Vec<Value> = Vec::with_capacity(messages.len());
    for m in &messages {
        if m.get("role").and_then(|r| r.as_str()) == Some("tool") {
            let tid = s(&m["tool_call_id"]);
            if tid.is_empty() || !declared.contains(&tid) {
                let content = match m.get("content").cloned().unwrap_or(Value::Null) {
                    Value::String(t) => {
                        Value::String(format!("[Orphaned tool output {tid}]: {t}"))
                    }
                    Value::Null => Value::String(format!("[Orphaned tool output {tid}]")),
                    other => other,
                };
                norm.push(json!({"role": "user", "content": content}));
                continue;
            }
        }
        norm.push(m.clone());
    }
    // 第二遍：assistant tool_calls 缺 output → 緊跟其後補佔位
    let mut out: Vec<Value> = Vec::with_capacity(norm.len() + 2);
    let mut i = 0;
    while i < norm.len() {
        let m = &norm[i];
        let is_asst = m.get("role").and_then(|r| r.as_str()) == Some("assistant");
        // 空 tool_calls 數組：移除字段後照常輸出
        if is_asst
            && m.get("tool_calls")
                .and_then(|c| c.as_array())
                .is_some_and(|a| a.is_empty())
        {
            let mut fixed = m.clone();
            if let Some(obj) = fixed.as_object_mut() {
                obj.remove("tool_calls");
            }
            out.push(fixed);
            i += 1;
            continue;
        }
        let mut calls: Vec<String> = vec![];
        let mut fixed: Option<Value> = None;
        if is_asst {
            if let Some(arr) = m.get("tool_calls").and_then(|c| c.as_array()) {
                if !arr.is_empty() {
                    // 空 id 補生成並同步改寫，保證配對一致
                    let mut rewritten = arr.clone();
                    let mut touched = false;
                    for (k, c) in rewritten.iter_mut().enumerate() {
                        if s(&c["id"]).is_empty() {
                            let gen = format!("hist_restored_{i}_{k}");
                            if let Some(obj) = c.as_object_mut() {
                                obj.insert("id".to_string(), Value::String(gen.clone()));
                            }
                            calls.push(gen);
                            touched = true;
                        } else {
                            calls.push(s(&c["id"]));
                        }
                    }
                    if touched {
                        let mut msg = m.clone();
                        if let Some(obj) = msg.as_object_mut() {
                            obj.insert("tool_calls".to_string(), Value::Array(rewritten));
                        }
                        fixed = Some(msg);
                    }
                }
            }
        }
        out.push(fixed.unwrap_or_else(|| m.clone()));
        if !calls.is_empty() {
            // 收集緊跟的連續 tool 消息
            let mut present: HashSet<String> = HashSet::new();
            let mut j = i + 1;
            while j < norm.len()
                && norm[j].get("role").and_then(|r| r.as_str()) == Some("tool")
            {
                present.insert(s(&norm[j]["tool_call_id"]));
                out.push(norm[j].clone());
                j += 1;
            }
            for id in &calls {
                if !present.contains(id) {
                    out.push(json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "content": MISSING_TOOL_OUTPUT,
                    }));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    fill_empty_content(out)
}

/// 非 assistant 空消息佔位：部分嚴格上游（如 OpenRouter 後端 ModelRun）要求
/// 非 assistant 角色的 content 非空，空字串/缺失會 `message content is required
/// for non-assistant roles` 400（2026-09-24 codex 實例）。assistant 豁免
///（帶 tool_calls 的 content:null 與純空文本均為 OpenAI 合法形狀）。
const EMPTY_MESSAGE_PLACEHOLDER: &str = "[empty message — placeholder filled by gateway]";

/// 非 assistant 消息 content 為空（缺失/null/空字串/空數組）時補佔位；
/// tool 角色沿用 MISSING_TOOL_OUTPUT（保持配對語義），其餘用通用佔位。
fn fill_empty_content(messages: Vec<Value>) -> Vec<Value> {
    messages
        .into_iter()
        .map(|m| {
            let role = m
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            if role == "assistant" {
                return m;
            }
            let empty = match m.get("content") {
                None | Some(Value::Null) => true,
                Some(Value::String(t)) => t.trim().is_empty(),
                Some(Value::Array(a)) => a.iter().all(|part| {
                    part.as_str().is_some_and(|t| t.trim().is_empty())
                        || part
                            .get("text")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t.trim().is_empty())
                }),
                _ => true,
            };
            if !empty {
                return m;
            }
            let mut fixed = m;
            if let Some(obj) = fixed.as_object_mut() {
                let ph = if role == "tool" {
                    MISSING_TOOL_OUTPUT
                } else {
                    EMPTY_MESSAGE_PLACEHOLDER
                };
                obj.insert("content".to_string(), Value::String(ph.to_string()));
            }
            fixed
        })
        .collect()
}

/// Anthropic content blocks → OpenAI content（字符串或 parts 數組）。
/// thinking/redacted_thinking 丟棄（OpenAI 無對等物）；tool_result → tool 消息。
fn anthropic_content_to_openai(blocks: &[Value]) -> Value {
    let mut parts = vec![];
    for b in blocks {
        match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => parts.push(json!({"type": "text", "text": s(&b["text"])})),
            "image" => {
                let src = &b["source"];
                let media = s(&src["media_type"]);
                let data = s(&src["data"]);
                if !data.is_empty() {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:{media};base64,{data}")}
                    }));
                }
            }
            _ => {}
        }
    }
    if parts.len() == 1 && parts[0]["type"] == "text" {
        return parts[0]["text"].clone();
    }
    Value::Array(parts)
}

/// Anthropic 請求 → OpenAI Chat 請求。model 由調用方先映射好傳入。
pub(crate) fn anthropic_to_openai(req: &Value, model: &str) -> Value {
    let mut messages = vec![];
    // system 置頂為 system 消息
    if let Some(sys) = req.get("system") {
        let text = match sys {
            Value::String(t) => t.clone(),
            Value::Array(blocks) => {
                let parts: Vec<String> = blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .map(|b| s(&b["text"]))
                    .collect();
                parts.join("\n")
            }
            _ => String::new(),
        };
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    if let Some(arr) = req.get("messages").and_then(|m| m.as_array()) {
        for m in arr {
            let role = s(&m["role"]);
            match role.as_str() {
                "user" => {
                    // tool_result 塊單獨展開為 tool 消息，不混入 user content
                    let mut parts = vec![];
                    let mut results = vec![];
                    match &m["content"] {
                        Value::String(t) => parts.push(json!({"type": "text", "text": t})),
                        Value::Array(blocks) => {
                            for b in blocks {
                                match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                                    "tool_result" => results.push(b),
                                    _ => parts.push((*b).clone()),
                                }
                            }
                        }
                        _ => {}
                    }
                    if !parts.is_empty() {
                        let content = if parts.len() == 1
                            && parts[0].get("type").and_then(|t| t.as_str()) == Some("text")
                        {
                            parts[0]["text"].clone()
                        } else {
                            // 仅保留 text/image
                            let kept: Vec<Value> = parts
                                .into_iter()
                                .filter(|p| {
                                    matches!(
                                        p.get("type").and_then(|t| t.as_str()),
                                        Some("text") | Some("image")
                                    )
                                })
                                .collect();
                            if kept.is_empty() {
                                Value::Array(vec![])
                            } else {
                                anthropic_content_to_openai(&kept)
                            }
                        };
                        messages.push(json!({"role": "user", "content": content}));
                    }
                    for b in results {
                        let content = match &b["content"] {
                            Value::String(t) => t.clone(),
                            Value::Array(ps) => ps
                                .iter()
                                .filter(|p| {
                                    p.get("type").and_then(|t| t.as_str()) == Some("text")
                                })
                                .map(|p| s(&p["text"]))
                                .collect::<Vec<_>>()
                                .join(""),
                            _ => String::new(),
                        };
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": s(&b["tool_use_id"]),
                            "content": content,
                        }));
                    }
                }
                "assistant" => {
                    let mut msg = serde_json::Map::new();
                    msg.insert("role".to_string(), Value::String("assistant".to_string()));
                    let mut texts = vec![];
                    let mut calls = vec![];
                    if let Some(blocks) = m.get("content").and_then(|c| c.as_array()) {
                        for b in blocks {
                            match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                                "text" => texts.push(s(&b["text"])),
                                "tool_use" => calls.push(json!({
                                    "id": s(&b["id"]),
                                    "type": "function",
                                    "function": {
                                        "name": s(&b["name"]),
                                        "arguments": match &b["input"] {
                                            Value::Object(_) | Value::Array(_) => {
                                                serde_json::to_string(&b["input"])
                                                    .unwrap_or_else(|_| "{}".to_string())
                                            }
                                            _ => "{}".to_string(),
                                        },
                                    }
                                })),
                                _ => {}
                            }
                        }
                    }
                    if !texts.is_empty() {
                        msg.insert("content".to_string(), Value::String(texts.join("")));
                    } else {
                        msg.insert("content".to_string(), Value::Null);
                    }
                    if !calls.is_empty() {
                        msg.insert("tool_calls".to_string(), Value::Array(calls));
                    }
                    messages.push(Value::Object(msg));
                }
                _ => {}
            }
        }
    }
    // 舊 session 續跑常見孤兒 tool_calls：缺 output 補佔位，保證上游配對校驗通過
    let messages = sanitize_chat_messages(messages);
    let mut out = serde_json::Map::new();
    out.insert("model".to_string(), Value::String(model.to_string()));
    out.insert("messages".to_string(), Value::Array(messages));
    // 直通的可選參數
    for (a, o) in [
        ("max_tokens", "max_tokens"),
        ("temperature", "temperature"),
        ("top_p", "top_p"),
        ("stream", "stream"),
    ] {
        if let Some(v) = req.get(a) {
            out.insert(o.to_string(), v.clone());
        }
    }
    if let Some(stop) = req.get("stop_sequences") {
        out.insert("stop".to_string(), stop.clone());
    }
    // tools / tool_choice
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let mapped: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": s(&t["name"]),
                        "description": s(&t["description"]),
                        "parameters": t.get("input_schema").cloned().unwrap_or(json!({"type": "object"})),
                    }
                })
            })
            .collect();
        out.insert("tools".to_string(), Value::Array(mapped));
    }
    if let Some(tc) = req.get("tool_choice") {
        let mapped = match tc.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "auto" => json!("auto"),
            "any" => json!("required"),
            "none" => json!("none"),
            "tool" => json!({"type": "function", "function": {"name": s(&tc["name"])}}),
            _ => json!("auto"),
        };
        out.insert("tool_choice".to_string(), mapped);
    }
    Value::Object(out)
}

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

fn data_line(event: &str, payload: &Value) -> String {
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

// ------------------------------------------------- Responses ⇄ Chat Completions ---

/// Codex 自訂（freeform）工具（如 `apply_patch`）被 function 化後，
/// 原始內容所放入的唯一參數名。
pub(crate) const CUSTOM_TOOL_INPUT_KEY: &str = "input";

/// 從 Responses 請求提取自訂工具名（`type == "custom"`）。
/// Codex 0.15x 把 apply_patch 宣告為 freeform 自訂工具；上游 chat 端不認
/// `type:"custom"` 會 400，故 function 化送出，回程再還原成 `custom_tool_call`。
pub(crate) fn custom_tool_names(req: &Value) -> Vec<String> {
    req.get("tools")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter(|t| t.get("type").and_then(|x| x.as_str()) == Some("custom"))
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                .filter(|n| !n.is_empty())
                .map(|n| n.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 自訂工具 function 化後的統一 parameters（單一 `input` 字串）。
fn custom_tool_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            CUSTOM_TOOL_INPUT_KEY: {
                "type": "string",
                "description": "Raw tool input, passed through verbatim.",
            }
        },
        "required": [CUSTOM_TOOL_INPUT_KEY],
        "additionalProperties": false,
    })
}

/// 上游 function 呼叫的 arguments（JSON）→ 自訂工具原始字串。
fn custom_tool_input(args: &str) -> String {
    serde_json::from_str::<Value>(args)
        .ok()
        .and_then(|v| {
            v.get(CUSTOM_TOOL_INPUT_KEY)
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| args.to_string())
}

/// reasoning item → 思考原文。優先解我們自己寫入的 `encrypted_content` 標記，
/// 其次取 Codex 存的 summary 文本（cc-switch 同款橋接：Codex 會把 summary
/// 原樣帶回，思考模型因此保有跨回合的計畫連續性）。
pub(crate) const REASONING_MARKER: &str = "tg-openai-reasoning-v1:reasoning:";

fn reasoning_text_of(item: &Value) -> Option<String> {
    if let Some(ec) = item.get("encrypted_content").and_then(|e| e.as_str()) {
        if let Some(rest) = ec.strip_prefix(REASONING_MARKER) {
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    for key in ["summary", "content"] {
        if let Some(parts) = item.get(key).and_then(|p| p.as_array()) {
            let text: String = parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// 工具完成項：自訂工具還原為 `custom_tool_call`，其餘維持 `function_call`。
fn tool_item(acc: &RToolAcc, cid: &str, is_custom: bool) -> Value {
    if is_custom {
        json!({
            "type": "custom_tool_call", "id": format!("ctc_{cid}"),
            "call_id": cid, "name": acc.name,
            "input": custom_tool_input(&acc.args), "status": "completed",
        })
    } else {
        json!({
            "type": "function_call", "id": format!("fc_{cid}"),
            "call_id": cid, "name": acc.name, "arguments": acc.args,
            "status": "completed",
        })
    }
}

fn is_custom_tool(names: &[String], name: &str) -> bool {
    names.iter().any(|n| n == name)
}

/// Responses 請求 → OpenAI Chat 請求。model 由調用方先映射好傳入。
/// 覆蓋 Codex 實際使用面：instructions、input（字符串/items）、function_call(_output)、
/// custom_tool_call(_output)、tools（扁平/自訂格式）、tool_choice、max_output_tokens、
/// reasoning.effort、溫度採樣與 stream；其餘欄位丟棄。
pub(crate) fn responses_to_openai(req: &Value, model: &str) -> Value {
    let mut messages: Vec<Value> = vec![];
    if let Some(sys) = req.get("instructions").and_then(|i| i.as_str()) {
        if !sys.is_empty() {
            messages.push(json!({"role": "system", "content": sys}));
        }
    }
    match req.get("input") {
        Some(Value::String(t)) => {
            if !t.is_empty() {
                messages.push(json!({"role": "user", "content": t.clone()}));
            }
        }
        Some(Value::Array(items)) => {
            // 連續的 function_call 合併為一條 assistant 消息（並行調用保持配對；
            // 逐條拆成多條 assistant 會被嚴格上游以「其後非 tool 消息」拒收）。
            // pending_reasoning：reasoning item 的思考，還原成 assistant 的
            // reasoning_content（思考模型靠它維持跨回合的計畫連續性）。
            let mut pending: Vec<Value> = vec![];
            let mut pending_reasoning: Option<String> = None;
            let flush_pending =
                |messages: &mut Vec<Value>, pending: &mut Vec<Value>, reasoning: &mut Option<String>| {
                    if !pending.is_empty() {
                        let mut m = json!({
                            "role": "assistant",
                            "content": null,
                            "tool_calls": std::mem::take(pending),
                        });
                        if let Some(rc) = reasoning.take() {
                            if !rc.is_empty() {
                                m["reasoning_content"] = Value::String(rc);
                            }
                        }
                        messages.push(m);
                    }
                };
            let attach_reasoning =
                |msg: &mut Value, reasoning: &mut Option<String>| {
                    if let Some(rc) = reasoning.take() {
                        if !rc.is_empty() {
                            msg["reasoning_content"] = Value::String(rc);
                        }
                    }
                };
            for it in items {
                let ty = it.get("type").and_then(|t| t.as_str()).unwrap_or("message");
                match ty {
                    "message" => {
                        flush_pending(&mut messages, &mut pending, &mut pending_reasoning);
                        let role = match s(&it["role"]).as_str() {
                            "developer" | "system" => "system",
                            "assistant" => "assistant",
                            _ => "user",
                        };
                        let content = match &it["content"] {
                            Value::String(t) => Value::String(t.clone()),
                            Value::Array(parts) => {
                                let texts: Vec<String> = parts
                                    .iter()
                                    .filter(|p| {
                                        matches!(
                                            p.get("type").and_then(|t| t.as_str()),
                                            Some("input_text") | Some("output_text")
                                                | Some("text")
                                        )
                                    })
                                    .map(|p| s(&p["text"]))
                                    .collect();
                                Value::String(texts.join(""))
                            }
                            _ => Value::String(String::new()),
                        };
                        let mut msg = json!({"role": role, "content": content});
                        if role == "assistant" {
                            attach_reasoning(&mut msg, &mut pending_reasoning);
                        } else {
                            // reasoning 不跨非 assistant 邊界
                            pending_reasoning = None;
                        }
                        messages.push(msg);
                    }
                    // reasoning item（Codex 存的思考）→ 掛到後續 assistant 訊息上
                    "reasoning" => {
                        pending_reasoning = reasoning_text_of(it);
                    }
                    "function_call" => {
                        pending.push(json!({
                            "id": s(&it["call_id"]),
                            "type": "function",
                            "function": {
                                "name": s(&it["name"]),
                                "arguments": s(&it["arguments"]),
                            }
                        }));
                    }
                    // Codex 自訂（freeform）工具歷史：還原成上游 function 呼叫
                    "custom_tool_call" => {
                        pending.push(json!({
                            "id": s(&it["call_id"]),
                            "type": "function",
                            "function": {
                                "name": s(&it["name"]),
                                "arguments": json!({CUSTOM_TOOL_INPUT_KEY: s(&it["input"])})
                                    .to_string(),
                            }
                        }));
                    }
                    "function_call_output" => {
                        flush_pending(&mut messages, &mut pending, &mut pending_reasoning);
                        let out = match &it["output"] {
                            Value::String(t) => t.clone(),
                            Value::Object(_) => {
                                it["output"]["output"].as_str().unwrap_or("").to_string()
                            }
                            _ => String::new(),
                        };
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": s(&it["call_id"]),
                            "content": out,
                        }));
                    }
                    "custom_tool_call_output" => {
                        flush_pending(&mut messages, &mut pending, &mut pending_reasoning);
                        let out = match &it["output"] {
                            Value::String(t) => t.clone(),
                            Value::Object(_) => {
                                it["output"]["output"].as_str().unwrap_or("").to_string()
                            }
                            _ => String::new(),
                        };
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": s(&it["call_id"]),
                            "content": out,
                        }));
                    }
                    _ => {}
                }
            }
            flush_pending(&mut messages, &mut pending, &mut pending_reasoning);
        }
        _ => {}
    }
    // 舊 session 續跑常見孤兒 function_call：缺 output 補佔位，保證上游配對校驗通過
    let messages = sanitize_chat_messages(messages);
    let mut out = serde_json::Map::new();
    out.insert("model".to_string(), Value::String(model.to_string()));
    out.insert("messages".to_string(), Value::Array(messages));
    for (a, o) in [
        ("max_output_tokens", "max_tokens"),
        ("temperature", "temperature"),
        ("top_p", "top_p"),
        ("stream", "stream"),
    ] {
        if let Some(v) = req.get(a) {
            if !v.is_null() {
                out.insert(o.to_string(), v.clone());
            }
        }
    }
    // 提示詞快取路由：Codex 的穩定工作階段 ID 直通（Zen 等相容上游用於
    // session 路由與前綴快取；嚴格上游若不認，既有 400 剝離機制會自動移除）
    if let Some(k) = req.get("prompt_cache_key").and_then(|v| v.as_str()) {
        if !k.is_empty() {
            out.insert(
                "prompt_cache_key".to_string(),
                Value::String(k.to_string()),
            );
        }
    }
    // reasoning.effort → reasoning_effort：只轉發上游目錄明示支援的等級，
    // 其餘（minimal/low 等）留空走上游預設，避免上游對不認識的值 400。
    if let Some(eff) = req
        .get("reasoning")
        .and_then(|r| r.get("effort"))
        .and_then(|e| e.as_str())
    {
        if matches!(eff, "medium" | "high" | "xhigh" | "max") {
            out.insert(
                "reasoning_effort".to_string(),
                Value::String(eff.to_string()),
            );
        }
    }
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let mut mapped: Vec<Value> = vec![];
        for t in tools {
            match t.get("type").and_then(|x| x.as_str()) {
                Some("function") => mapped.push(json!({
                    "type": "function",
                    "function": {
                        "name": s(&t["name"]),
                        "description": s(&t["description"]),
                        "parameters": t.get("parameters").cloned().unwrap_or(json!({"type": "object"})),
                    }
                })),
                // freeform 自訂工具（Codex apply_patch）：function 化送出，
                // 回程由 tool_item() 還原成 custom_tool_call
                Some("custom") => {
                    let name = s(&t["name"]);
                    if name.is_empty() {
                        continue;
                    }
                    let mut desc = s(&t["description"]);
                    if !desc.is_empty() {
                        desc.push_str("\n\n");
                    }
                    desc.push_str(
                        "Provide this tool's raw input content in the `input` string field.",
                    );
                    mapped.push(json!({
                        "type": "function",
                        "function": {
                            "name": name,
                            "description": desc,
                            "parameters": custom_tool_parameters(),
                        }
                    }));
                }
                _ => {}
            }
        }
        if !mapped.is_empty() {
            out.insert("tools".to_string(), Value::Array(mapped));
        }
    }
    if let Some(tc) = req.get("tool_choice") {
        let mapped = match tc {
            Value::String(_) => tc.clone(),
            Value::Object(_) => {
                if tc.get("type").and_then(|x| x.as_str()) == Some("function") {
                    json!({"type": "function", "function": {"name": s(&tc["name"])}})
                } else {
                    json!("auto")
                }
            }
            _ => json!("auto"),
        };
        out.insert("tool_choice".to_string(), mapped);
    }
    if let Some(p) = req.get("parallel_tool_calls") {
        if !p.is_null() {
            out.insert("parallel_tool_calls".to_string(), p.clone());
        }
    }
    Value::Object(out)
}

/// OpenAI Chat 非流式回包 → Responses response。echo_model 原樣回顯請求模型。
/// custom_tools：自訂工具名，命中者回程還原為 `custom_tool_call`。
pub(crate) fn openai_to_responses(
    resp: &Value,
    echo_model: &str,
    custom_tools: &[String],
) -> Value {
    let choice = resp.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first());
    let mut output: Vec<Value> = vec![];
    let mut status = "completed";
    if let Some(ch) = choice {
        let msg = &ch["message"];
        if let Some(rt) = msg.get("reasoning_content").and_then(|r| r.as_str()) {
            if !rt.is_empty() {
                output.push(json!({
                    "type": "reasoning", "id": format!("rs_{}", s(&resp["id"]).trim_start_matches("chatcmpl-")),
                    "summary": [{"type": "summary_text", "text": rt}],
                    "content": null,
                }));
            }
        }
        if let Some(t) = msg.get("content").and_then(|c| c.as_str()) {
            if !t.is_empty() {
                output.push(json!({
                    "type": "message",
                    "id": format!("msg_{}", s(&resp["id"]).trim_start_matches("chatcmpl-")),
                    "role": "assistant", "status": "completed",
                    "content": [{"type": "output_text", "text": t, "annotations": []}],
                }));
            }
        }
        if let Some(calls) = msg.get("tool_calls").and_then(|c| c.as_array()) {
            for call in calls {
                let acc = RToolAcc {
                    id: s(&call["id"]),
                    name: s(&call["function"]["name"]),
                    args: s(&call["function"]["arguments"]),
                };
                let cid = if acc.id.is_empty() {
                    format!("call_{}", output.len())
                } else {
                    acc.id.clone()
                };
                output.push(tool_item(&acc, &cid, is_custom_tool(custom_tools, &acc.name)));
            }
        }
        if ch.get("finish_reason").and_then(|f| f.as_str()) == Some("length") {
            status = "incomplete";
        }
    }
    let u = resp.get("usage");
    let inp = u.and_then(|x| x.get("prompt_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
    let outp = u.and_then(|x| x.get("completion_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
    let cached = u.and_then(|x| x.get("prompt_tokens_details"))
        .and_then(|x| x.get("cached_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
    let mut body = json!({
        "id": format!("resp_{}", s(&resp["id"]).trim_start_matches("chatcmpl-")),
        "object": "response",
        "created_at": resp.get("created").and_then(|c| c.as_i64()).unwrap_or(0),
        "status": status,
        "model": echo_model,
        "output": output,
        "parallel_tool_calls": true,
        "tool_choice": "auto",
        "tools": [],
        "usage": {
            "input_tokens": inp,
            "input_tokens_details": {"cached_tokens": cached},
            "output_tokens": outp,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": inp + outp,
        },
    });
    if status == "incomplete" {
        body["incomplete_details"] = json!({"reason": "max_output_tokens"});
    }
    body
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn responses_skeleton(id: &str, model: &str) -> Value {
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
    created: bool,
    completed: bool,
    failed: bool,
    saw_finish: bool,
    saw_done: bool,
    text_open: bool,
    reasoning_open: bool,
    msg_id: String,
    resp_id: String,
    full_text: String,
    reasoning_text: String,
    tools: std::collections::HashMap<i64, RToolAcc>,
    tool_order: Vec<i64>,
    next_idx: i64,
}

impl ResponsesStreamState {
    /// 上游送出 `[DONE]` 時標記：有它即視為正常收尾（部分上游不發 finish_reason）。
    pub(crate) fn note_done(&mut self) {
        self.saw_done = true;
    }

    fn reasoning_id(&self) -> String {
        format!("{}_rs", self.msg_id)
    }

    /// reasoning 收尾：summary_text.done → part.done → output_item.done。
    /// 思考以 summary 事件送出（Codex 只認 reasoning_summary_*，不認 reasoning_text_*）。
    fn close_reasoning(&mut self) -> Vec<String> {
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

#[derive(Debug, Default)]
struct RToolAcc {
    id: String,
    name: String,
    args: String,
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn prompt_cache_key_passthrough() {
        let req: Value =
            serde_json::from_str(r#"{"model": "m", "prompt_cache_key": "sess-123"}"#).unwrap();
        assert_eq!(responses_to_openai(&req, "m")["prompt_cache_key"], "sess-123");
        let req: Value = serde_json::from_str(r#"{"model": "m"}"#).unwrap();
        assert!(responses_to_openai(&req, "m").get("prompt_cache_key").is_none());
    }
}
