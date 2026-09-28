//! OpenAI 兼容上游的消息配對修復：合併分裂的 assistant、補缺失的 tool 輸出、
//! 孤兒 tool 轉 user、空內容補佔位。Chat 與 Responses 兩個方向共用。

use super::*;
use serde_json::json;

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
