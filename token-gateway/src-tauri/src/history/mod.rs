//! M5：歷史回填 — 離線解析本機會話並冪等寫入 `request_logs`。
//!
//! 來源有三：Claude Code（`~/.claude/projects/**/*.jsonl`，按 assistant 訊息計）、
//! Codex（`~/.codex/sessions/**/*.jsonl`，按 `token_usage_record` 計）、
//! OpenCode（`~/.local/share/opencode/opencode.db` 的 `session` 表，按會話計）。
//!
//! 寫入列標 `source='import'`、`import_path=來源文件`；key/provider 未知記 NULL
//! （OpenCode 會話按 `providerID` 名稱回填渠道，能對上才寫）。
//!
//! 冪等：`import_state` 記每文件 (mtime, size)／整庫指紋；
//! 指紋變化時按 `import_path` 刪除重插。與網關日誌的疑似重複
//! （同 app+模型+輸入輸出+時間 ±120s）跳過並計數。
//!
//! 費用：回填當下按全域/渠道定價快照計算並凍存；無價模型記 0 並列入摘要。
//!
//! 本模組原本是單一檔案（1,038 行），現依主題拆成子模組：
//! - `scan`：掃描各工具來源（`ScanTool` / `scan_history`）
//! - `parse`：三種來源格式的解析（claude / codex / opencode）
//! - `commit`：寫入、判重、定價判定與孤兒清理
//! - `import`：回填主入口（`ImportSummary` 等對外型別）
//! - `tests`：單元測試（僅測試組建）
//!
//! 原檔「工具函數」段的共用項目（`ParsedRow`、`num`、`str_field`、`parse_ts`、
//! `file_fp`、`collect_jsonl_files`）與兩個常數留在本檔，子模組以
//! `use super::…` 取用；`crate::history::X` 這個路徑對所有對外名稱都不變
//! （`import_history`、`scan_history`、`IMPORT_SOURCE`、`ScanTool`、
//! `ToolImportStat`、`UnpricedModel`、`ImportSummary`）。

use std::path::{Path, PathBuf};

mod commit;
mod import;
mod parse;
mod scan;

#[cfg(test)]
mod tests;

pub use import::*;
pub use scan::*;

pub const IMPORT_SOURCE: &str = "import";
/// 與網關日誌疑似重複的時間容差（毫秒）。
const DUPE_WINDOW_MS: i64 = 120_000;

struct ParsedRow {
    app: &'static str,
    model_raw: String,
    provider_id: Option<i64>,
    in_tok: i64,
    out_tok: i64,
    cache_read: i64,
    cache_write: i64,
    ts_ms: i64,
    import_path: String,
}

// ---------------------------------------------------------------- 工具函數 ---

fn num(v: &serde_json::Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|n| n.as_i64())
        .unwrap_or(0)
        .max(0)
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// RFC3339 → 毫秒時間戳；失敗回 None。
fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.timestamp_millis())
}

fn file_fp(path: &Path) -> (i64, i64) {
    let md = std::fs::metadata(path).ok();
    let size = md.as_ref().map(|m| m.len() as i64).unwrap_or(0);
    let mtime = md
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (mtime, size)
}

/// 遞迴收集目錄下所有 `.jsonl`（大小寫不敏感）。
fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect_jsonl_files(&p, out);
        } else if p
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jsonl"))
        {
            out.push(p);
        }
    }
}
