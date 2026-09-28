//! 掃描各工具的來源（原 `history.rs` 的「掃描」段）。

use rusqlite::OpenFlags;
use serde::Serialize;
use std::path::{Path, PathBuf};

use super::collect_jsonl_files;

#[derive(Debug, Clone, Serialize)]
pub struct ScanTool {
    pub tool: String,
    pub files: usize,
    pub bytes: u64,
    /// 僅 opencode：會話數。
    pub sessions: usize,
}

// ---------------------------------------------------------------- 掃描 ---

/// 各工具待掃描的來源（不存在的目錄回空列表，不報錯）。
pub fn scan_history(home: &Path) -> Vec<ScanTool> {
    let mut tools = Vec::new();
    let mut claude_files = Vec::new();
    collect_jsonl_files(&home.join(".claude").join("projects"), &mut claude_files);
    tools.push(summarize("claude", claude_files, 0));
    let mut codex_files = Vec::new();
    collect_jsonl_files(&home.join(".codex").join("sessions"), &mut codex_files);
    tools.push(summarize("codex", codex_files, 0));
    // opencode：單庫 + 會話數（唯讀 COUNT，失敗當 0）
    let oc = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    let (files, bytes, sessions) = if oc.exists() {
        let n = rusqlite::Connection::open_with_flags(&oc, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| {
                c.query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
                    .ok()
            })
            .unwrap_or(0);
        (1, std::fs::metadata(&oc).map(|m| m.len()).unwrap_or(0), n as usize)
    } else {
        (0, 0, 0)
    };
    tools.push(ScanTool {
        tool: "opencode".into(),
        files,
        bytes,
        sessions,
    });
    tools
}

fn summarize(tool: &str, files: Vec<PathBuf>, sessions: usize) -> ScanTool {
    let bytes = files
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();
    ScanTool {
        tool: tool.into(),
        files: files.len(),
        bytes,
        sessions,
    }
}
