//! Anthropic Messages 請求 → OpenAI Chat Completions 請求（請求方向）。

use super::*;
use serde_json::json;

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
