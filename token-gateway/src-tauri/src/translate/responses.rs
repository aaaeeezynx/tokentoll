//! Responses ⇄ OpenAI Chat Completions：請求側（`responses_to_openai`）與
//! 非流式回包（`openai_to_responses`）。

use super::*;
use serde_json::{json, Value};

use super::responses_tools::{
    custom_tool_parameters, is_custom_tool, reasoning_text_of, tool_item, RToolAcc,
};

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
