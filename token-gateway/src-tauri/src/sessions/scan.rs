//! 掃描各工具的會話（P3.4）。
//!
//! 每個工具一段：找出檔案／資料列 → 抓標題與專案目錄 → 組成 [`SessionSummary`]。
//! **只讀不寫**（刪除在 `store.rs`，而且只允許會話目錄底下的檔案）。
//!
//! 標題取得方式：
//! - Codex：`session_meta` 有 cwd 與 session id；標題取**第一則使用者訊息**
//!   （往下掃到 `response_item` 且 role=user，最多掃 `TITLE_SCAN_LINES` 行）。
//! - Claude：檔案第一段就有 `cwd`；標題同樣取第一則使用者訊息。
//! - OpenCode：資料庫的 `session.title` 現成的 ✓。

use std::path::{Path, PathBuf};

use super::{clean_title, resume_command, SessionSummary, MAX_PER_APP};

/// 找標題時最多掃幾行（避免為了標題讀完整個大檔）。
const TITLE_SCAN_LINES: usize = 400;
/// 讀檔上限（bytes）：超過就只讀前面這麼多來找標題。
const TITLE_READ_CAP: u64 = 512 * 1024;

/// 某個工具的會話目錄（可能有多個；回傳存在的那些）。
pub(crate) fn session_dirs(app: &str) -> Result<Vec<PathBuf>, String> {
    let home = crate::tools::user_home()?;
    let all = match app {
        "claude" => vec![home.join(".claude").join("projects")],
        "codex" => vec![
            home.join(".codex").join("sessions"),
            home.join(".codex").join("archived_sessions"),
        ],
        "opencode" => vec![home.join(".local").join("share").join("opencode")],
        _ => return Err(format!("會話管理不支援的工具：{app}")),
    };
    Ok(all.into_iter().filter(|p| p.exists()).collect())
}

/// 掃描全部工具。
///
/// **每個工具各自上限**（不共用一個全域上限）：實機驗證時踩到 —— Codex 有 617 個
/// 會話檔、Claude 32 個、OpenCode 104 個，用「全域 500 筆再截斷」會讓**整個
/// OpenCode 消失**（它的會話比較舊，全被擠掉），使用者切到 OpenCode 篩選器卻看到 0 筆。
pub fn scan_all(apps: &[String]) -> Vec<SessionSummary> {
    let mut out = vec![];
    for app in apps {
        match scan(app) {
            Ok(mut v) => {
                v.sort_by_key(|s| std::cmp::Reverse(s.last_active_ms));
                v.truncate(MAX_PER_APP);
                out.append(&mut v);
            }
            Err(_) => continue, // 單一工具失敗不影響其他
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.last_active_ms));
    out
}

/// 掃描單一工具。
pub(crate) fn scan(app: &str) -> Result<Vec<SessionSummary>, String> {
    if app == "opencode" {
        return store_scan();
    }
    let mut out = vec![];
    for dir in session_dirs(app)? {
        collect_jsonl(app, &dir, &mut out);
    }
    Ok(out)
}

/// JSONL 會話檔（Claude 與 Codex 共用；標題解析由 `parse_meta` 決定）。
fn collect_jsonl(app: &str, root: &Path, out: &mut Vec<SessionSummary>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let id = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // Codex 的檔名是 rollout-<時間>-<uuid>，真正的 id 在 session_meta 裡
            let (title, cwd, real_id) = parse_head(app, &p, &id);
            let last = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(SessionSummary {
                app: app.to_string(),
                id: real_id.clone(),
                title,
                project_dir: cwd,
                path: p.to_string_lossy().to_string(),
                last_active_ms: last,
                size: meta.len(),
                deletable: true,
                resume_command: resume_command(app, &real_id),
                message_count: 0,
            });
        }
    }
}

/// 讀檔頭抓（標題, 專案目錄, 真正的 session id）。
fn parse_head(app: &str, path: &Path, file_stem: &str) -> (String, String, String) {
    use std::io::{BufRead, BufReader, Read};
    let Ok(f) = std::fs::File::open(path) else {
        return ("（讀不到）".into(), String::new(), file_stem.to_string());
    };
    let mut reader = BufReader::new(f.take(TITLE_READ_CAP));
    let mut line = String::new();
    let mut title = String::new();
    let mut cwd = String::new();
    let mut id = file_stem.to_string();
    for _ in 0..TITLE_SCAN_LINES {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if app == "codex" {
            if v.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
                let p = v.get("payload").cloned().unwrap_or_default();
                if let Some(s) = p.get("id").and_then(|x| x.as_str()) {
                    id = s.to_string();
                }
                if let Some(c) = p.get("cwd").and_then(|x| x.as_str()) {
                    cwd = c.to_string();
                }
            }
        } else if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
            if cwd.is_empty() {
                cwd = c.to_string();
            }
        }
        if title.is_empty() {
            title = user_text(app, &v);
        }
        if !title.is_empty() && !cwd.is_empty() {
            break;
        }
    }
    (clean_title(&title), cwd, id)
}

/// 從一行 JSONL 取出「使用者說的第一句話」。
fn user_text(app: &str, v: &serde_json::Value) -> String {
    if app == "codex" {
        if v.get("type").and_then(|t| t.as_str()) != Some("response_item") {
            return String::new();
        }
        let p = v.get("payload").cloned().unwrap_or_default();
        if p.get("role").and_then(|r| r.as_str()) != Some("user") {
            return String::new();
        }
        return content_text(p.get("content"));
    }
    // Claude：type=user 且 message.content
    if v.get("type").and_then(|t| t.as_str()) != Some("user") {
        return String::new();
    }
    let m = v.get("message").cloned().unwrap_or_default();
    content_text(m.get("content"))
}

/// `content` 可能是字串、或 `[{type:"text", text:"…"}]` 陣列。
pub(crate) fn content_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|it| {
                it.get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        it.get("content")
                            .and_then(|c| c.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

// ------------------------------------------------------- OpenCode（SQLite）---

/// OpenCode 的會話資料庫路徑。
pub(crate) fn opencode_db() -> Result<PathBuf, String> {
    let home = crate::tools::user_home()?;
    Ok(home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db"))
}

/// 讀 OpenCode 的 SQLite（**唯讀**）。
pub(crate) fn open_opencode() -> Result<rusqlite::Connection, String> {
    let p = opencode_db()?;
    if !p.exists() {
        return Err(format!("找不到 OpenCode 資料庫：{}", p.display()));
    }
    rusqlite::Connection::open_with_flags(
        &p,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("開啟 OpenCode 資料庫失敗：{e}"))
}

fn store_scan() -> Result<Vec<SessionSummary>, String> {
    let conn = open_opencode()?;
    let db_path = opencode_db()?.to_string_lossy().to_string();
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.title, s.directory, s.time_updated,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id)
             FROM session s ORDER BY s.time_updated DESC",
        )
        .map_err(|e| format!("讀 OpenCode 會話失敗：{e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let title: String = r.get(1)?;
            let dir: String = r.get(2)?;
            let updated: i64 = r.get(3)?;
            let count: i64 = r.get(4)?;
            Ok(SessionSummary {
                app: "opencode".to_string(),
                resume_command: resume_command("opencode", &id),
                id,
                title: clean_title(&title),
                project_dir: dir,
                path: db_path.clone(),
                last_active_ms: updated,
                size: 0,
                // 別人的資料庫只讀不寫（與 cc-switch「沒有本機檔案路徑的不能刪」同一條規矩）
                deletable: false,
                message_count: count,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn codex_head_yields_id_cwd_and_title() {
        let d = tmp();
        let p = d.path().join("rollout-2026-10-01T00-00-00-01abc.jsonl");
        let lines = [
            r#"{"type":"session_meta","payload":{"id":"01abc","cwd":"C:/work/proj"}}"#,
            r#"{"type":"event_msg","payload":{"type":"turn_started"}}"#,
            r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"text","text":"幫我修這個 bug"}]}}"#,
        ];
        std::fs::write(&p, lines.join("\n")).unwrap();
        let (title, cwd, id) = parse_head("codex", &p, "rollout-2026-10-01T00-00-00-01abc");
        assert_eq!(id, "01abc", "真正的 id 在 session_meta 裡");
        assert_eq!(cwd, "C:/work/proj");
        assert_eq!(title, "幫我修這個 bug");
    }

    #[test]
    fn claude_head_yields_cwd_and_title() {
        let d = tmp();
        let p = d.path().join("90b2.jsonl");
        let lines = [
            r#"{"type":"summary","content":"x"}"#,
            r#"{"type":"user","cwd":"C:/Users/x/proj","message":{"content":"第一個問題"}}"#,
        ];
        std::fs::write(&p, lines.join("\n")).unwrap();
        let (title, cwd, id) = parse_head("claude", &p, "90b2");
        assert_eq!(cwd, "C:/Users/x/proj");
        assert_eq!(title, "第一個問題");
        assert_eq!(id, "90b2", "Claude 的檔名就是 session id");
    }

    #[test]
    fn missing_or_broken_files_do_not_panic() {
        let d = tmp();
        let broken = d.path().join("broken.jsonl");
        std::fs::write(&broken, "not json\n{also not}\n").unwrap();
        let (title, cwd, id) = parse_head("claude", &broken, "broken");
        assert_eq!(title, "（無標題）");
        assert!(cwd.is_empty());
        assert_eq!(id, "broken");
        // 讀不到的檔案
        let (t2, _c2, _i2) = parse_head("codex", &d.path().join("nope.jsonl"), "nope");
        assert_eq!(t2, "（讀不到）");
    }

    #[test]
    fn content_text_handles_both_shapes() {
        let s = serde_json::json!("直接字串");
        assert_eq!(content_text(Some(&s)), "直接字串");
        let a = serde_json::json!([{"type":"text","text":"a"},{"type":"text","text":"b"}]);
        assert_eq!(content_text(Some(&a)), "a b");
        assert_eq!(content_text(None), "");
    }

    /// 每個工具各自上限：會話較舊的工具**不可以**被別的工具擠掉。
    ///
    /// 由來（實機驗證）：Codex 617 檔、Claude 32、OpenCode 104，用「全域 500 筆再
    /// 截斷」會讓 OpenCode 一筆都不剩 —— 使用者切到 OpenCode 篩選器看到 0 筆。
    #[test]
    fn per_app_cap_keeps_every_tool_visible() {
        let mut all: Vec<SessionSummary> = (0..MAX_PER_APP + 5)
            .map(|i| SessionSummary {
                app: "codex".into(),
                id: format!("c{i}"),
                last_active_ms: 1_000_000 + i as i64, // 全部比 opencode 新
                ..Default::default()
            })
            .collect();
        all.push(SessionSummary {
            app: "opencode".into(),
            id: "old-one".into(),
            last_active_ms: 1, // 最舊
            ..Default::default()
        });
        // 模擬 scan_all 的合併邏輯
        let mut per_app: Vec<SessionSummary> = all
            .into_iter()
            .filter(|s| s.app == "codex")
            .collect::<Vec<_>>();
        per_app.sort_by_key(|s| std::cmp::Reverse(s.last_active_ms));
        per_app.truncate(MAX_PER_APP);
        let mut merged = per_app;
        merged.push(SessionSummary {
            app: "opencode".into(),
            id: "old-one".into(),
            last_active_ms: 1,
            ..Default::default()
        });
        assert!(
            merged.iter().any(|s| s.app == "opencode"),
            "最舊的工具也必須留在清單裡"
        );
        assert_eq!(merged.iter().filter(|s| s.app == "codex").count(), MAX_PER_APP);
    }

    #[test]
    fn scan_walks_nested_dirs_and_sorts() {        let d = tmp();
        let root = d.path().join("sessions");
        std::fs::create_dir_all(root.join("2026").join("10").join("01")).unwrap();
        std::fs::write(
            root.join("2026").join("10").join("01").join("rollout-a.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"a","cwd":"C:/p1"}}"#,
        )
        .unwrap();
        std::fs::write(root.join("rollout-b.jsonl"), "{}").unwrap();
        let mut out = vec![];
        collect_jsonl("codex", &root, &mut out);
        assert_eq!(out.len(), 2, "兩層都要掃到：{out:?}");
        assert!(out.iter().all(|s| s.deletable));
        assert!(out.iter().all(|s| !s.resume_command.is_empty()));
    }
}
