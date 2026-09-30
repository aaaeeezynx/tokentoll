//! 讀一則會話的對話內容（P3.4）。
//!
//! 三種來源各自的形狀（都是實地看過的）：
//! - **Codex**：`response_item` 且 `payload.role` 是 user／assistant，
//!   內容在 `payload.content[]`（`{type:"input_text"|"output_text", text}`）。
//! - **Claude**：JSONL 每行的 `type` 是 user／assistant，內容在 `message.content`
//!   （字串或 `[{type:"text",text}]`）。
//! - **OpenCode**：SQLite 的 `message`（role）＋ `part`（文字片段）。

use super::scan::{content_text, open_opencode};
use super::{SessionMessage, MAX_MESSAGES};

/// 讀某個工具的某一則會話（`path` 是掃描時回報的來源路徑）。
pub fn read_transcript(
    app: &str,
    session_id: &str,
    path: &str,
) -> Result<Vec<SessionMessage>, String> {
    match app {
        "opencode" => read_opencode(session_id),
        "claude" | "codex" => read_jsonl(app, path),
        _ => Err(format!("會話管理不支援的工具：{app}")),
    }
}

fn read_jsonl(app: &str, path: &str) -> Result<Vec<SessionMessage>, String> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path).map_err(|e| format!("讀取會話檔失敗：{e}"))?;
    let mut out = vec![];
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(m) = parse_line(app, &v) {
            out.push(m);
        }
    }
    Ok(cap(out))
}

/// 一行 JSONL → 一則訊息（不是訊息就 `None`）。
fn parse_line(app: &str, v: &serde_json::Value) -> Option<SessionMessage> {
    let ts = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(parse_ts)
        .unwrap_or(0);
    if app == "codex" {
        if v.get("type").and_then(|t| t.as_str()) != Some("response_item") {
            return None;
        }
        let p = v.get("payload")?;
        let role = p.get("role").and_then(|r| r.as_str()).unwrap_or("");
        // 工具呼叫（function_call 等）也算一則，讓時間軸完整
        let text = match p.get("type").and_then(|t| t.as_str()) {
            Some("message") => content_text(p.get("content")),
            Some(t) if t.contains("call") => {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or(t);
                format!("[{t}] {name}")
            }
            _ => String::new(),
        };
        if text.trim().is_empty() {
            return None;
        }
        let role = match role {
            "user" => "user",
            "assistant" => "assistant",
            "system" => "system",
            _ => "tool",
        };
        return Some(SessionMessage {
            role: role.to_string(),
            text,
            ts,
        });
    }
    // Claude
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ty != "user" && ty != "assistant" && ty != "system" {
        return None;
    }
    let m = v.get("message").cloned().unwrap_or_default();
    let is_tool = m
        .get("content")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter().any(|x| {
                x.get("type")
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| t.contains("tool_"))
            })
        })
        .unwrap_or(false);
    let text = content_text(m.get("content"));
    if text.trim().is_empty() {
        return None;
    }
    Some(SessionMessage {
        role: if is_tool { "tool".into() } else { ty.to_string() },
        text,
        ts,
    })
}

fn read_opencode(session_id: &str) -> Result<Vec<SessionMessage>, String> {
    let conn = open_opencode()?;
    let mut stmt = conn
        .prepare(
            "SELECT m.time_created, m.data,
                    (SELECT GROUP_CONCAT(p.data, char(10)) FROM part p
                     WHERE p.message_id = m.id ORDER BY p.time_created)
             FROM message m WHERE m.session_id = ?1 ORDER BY m.time_created ASC",
        )
        .map_err(|e| format!("讀 OpenCode 訊息失敗：{e}"))?;
    let rows = stmt
        .query_map([session_id], |r| {
            let ts: i64 = r.get(0)?;
            let data: String = r.get(1)?;
            let parts: Option<String> = r.get(2)?;
            Ok((ts, data, parts))
        })
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    for (ts, data, parts) in rows.filter_map(|r| r.ok()) {
        let meta: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
        let role = meta
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("assistant")
            .to_string();
        // part 的資料列是一行一個 JSON，把裡面的 text 抓出來
        let text = parts
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|v| {
                v.get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            continue;
        }
        let role = match role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            "system" => "system",
            _ => "tool",
        };
        out.push(SessionMessage {
            role: role.to_string(),
            text,
            ts,
        });
    }
    Ok(cap(out))
}

/// 太長的對話只留最後 `MAX_MESSAGES` 則（前面的脈絡對瀏覽沒用）。
fn cap(mut v: Vec<SessionMessage>) -> Vec<SessionMessage> {
    if v.len() > MAX_MESSAGES {
        v.drain(..v.len() - MAX_MESSAGES);
    }
    v
}

/// ISO-8601 → 毫秒。
fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_lines_become_messages() {
        let user = serde_json::json!({
            "timestamp": "2026-10-01T00:00:00Z",
            "type": "response_item",
            "payload": {"type": "message", "role": "user",
                        "content": [{"type": "input_text", "text": "問題一"}]}
        });
        let m = parse_line("codex", &user).unwrap();
        assert_eq!(m.role, "user");
        assert_eq!(m.text, "問題一");
        assert!(m.ts > 0, "時間戳要解析出來");
        // 非訊息的列要忽略
        assert!(parse_line("codex", &serde_json::json!({"type":"event_msg"})).is_none());
        // 工具呼叫也要進時間軸
        let call = serde_json::json!({
            "type": "response_item",
            "payload": {"type": "function_call", "name": "shell"}
        });
        let m = parse_line("codex", &call).unwrap();
        assert_eq!(m.role, "tool");
        assert!(m.text.contains("shell"));
    }

    #[test]
    fn claude_lines_become_messages() {
        let u = serde_json::json!({
            "type": "user", "timestamp": "2026-10-01T00:00:01Z",
            "message": {"content": "你好"}
        });
        let m = parse_line("claude", &u).unwrap();
        assert_eq!((m.role.as_str(), m.text.as_str()), ("user", "你好"));
        let a = serde_json::json!({
            "type": "assistant",
            "message": {"content": [{"type": "text", "text": "回應內容"}]}
        });
        assert_eq!(parse_line("claude", &a).unwrap().text, "回應內容");
        // 工具結果 → role=tool
        let t = serde_json::json!({
            "type": "user",
            "message": {"content": [{"type": "tool_result", "content": "結果"}]}
        });
        assert_eq!(parse_line("claude", &t).unwrap().role, "tool");
        // summary 之類不是訊息
        assert!(parse_line("claude", &serde_json::json!({"type":"summary"})).is_none());
    }

    #[test]
    fn empty_content_is_skipped() {
        let v = serde_json::json!({
            "type": "assistant",
            "message": {"content": [{"type": "text", "text": "   "}]}
        });
        assert!(parse_line("claude", &v).is_none());
    }

    #[test]
    fn cap_keeps_the_tail() {
        let many: Vec<SessionMessage> = (0..MAX_MESSAGES + 20)
            .map(|i| SessionMessage {
                role: "user".into(),
                text: format!("m{i}"),
                ts: 0,
            })
            .collect();
        let out = cap(many);
        assert_eq!(out.len(), MAX_MESSAGES);
        assert_eq!(out.last().unwrap().text, format!("m{}", MAX_MESSAGES + 19));
    }

    #[test]
    fn unknown_app_is_an_error() {
        assert!(read_transcript("gemini", "x", "/tmp/nope").is_err());
    }
}
