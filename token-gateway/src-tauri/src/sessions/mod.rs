//! 會話管理（P3.4，對標 cc-switch 的 Session Manager）。
//!
//! ## cc-switch 的模型（照它手冊 §3.4 抄）
//!
//! 一個地方瀏覽／搜尋／管理各工具的對話：左邊清單（搜尋＋依工具篩選＋重新整理），
//! 右邊詳情（標題、最後活動時間、專案目錄、**續聊指令**、對話全文）。
//! 動作有：續聊（非 macOS 是**複製指令到剪貼簿**）、刪除（單筆／批次，刪前確認）、
//! 依最後活動時間排序。
//!
//! ## 各工具的會話存在哪（實地確認過）
//!
//! | 工具 | 位置 | 形態 |
//! |---|---|---|
//! | Claude Code | `~/.claude/projects/<專案>/<uuid>.jsonl` | JSONL（`type: user/assistant` ＋ `message`） |
//! | Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` ＋ `archived_sessions/` | JSONL（第一行 `session_meta`，其後 `response_item`） |
//! | OpenCode | `~/.local/share/opencode/opencode.db` | **SQLite**（`session`／`message`／`part` 三張表） |
//!
//! ## 刪除的界線
//!
//! - Claude／Codex 是檔案 → 可以刪（**只刪該工具會話目錄底下的檔案**，路徑會先檢查）。
//! - OpenCode 是**別人的資料庫** → 我們**只讀不寫**（不刪）。這與 cc-switch 自己的
//!   「沒有本機來源路徑的會話不能刪」是同一條規矩。
//!
//! 另外：刪掉會話檔**不會**撤銷之前回填的用量統計（用量是帳，不隨對話消失）——
//! UI 會把這件事講清楚。

pub(crate) mod read;
pub(crate) mod scan;
pub(crate) mod store;

pub use read::read_transcript;
pub use scan::scan_all;
pub use store::{delete_sessions, DeleteOutcome, SessionEnv};

use serde::{Deserialize, Serialize};

use crate::tools::TAKEOVER_APPS;

/// 對話裡的一則訊息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionMessage {
    /// user | assistant | system | tool
    pub role: String,
    pub text: String,
    /// 毫秒時間戳（沒有就 0）
    pub ts: i64,
}

/// 清單裡的一則會話。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionSummary {
    /// claude | codex | opencode
    pub app: String,
    pub id: String,
    pub title: String,
    pub project_dir: String,
    /// 來源檔（OpenCode 是資料庫，這裡放資料庫路徑，並把 deletable 設 false）
    pub path: String,
    pub last_active_ms: i64,
    pub size: u64,
    /// 能不能刪（本機檔案才可）
    pub deletable: bool,
    /// 續聊指令（空＝這筆不支援續聊）
    pub resume_command: String,
    pub message_count: i64,
}

/// 這個工具有沒有支援會話管理。
pub fn supported_apps() -> Vec<&'static str> {
    TAKEOVER_APPS.to_vec()
}

/// **每個工具**的會話掃描上限（新到舊）。
///
/// 刻意不做「全域上限」：那會讓會話較舊的工具整個消失（實機驗證踩過）。
pub(crate) const MAX_PER_APP: usize = 300;
/// 讀取單一會話時的訊息上限（很長的對話只取最後這些則 + 前面幾則）。
pub(crate) const MAX_MESSAGES: usize = 400;

/// 續聊指令（實地確認過：`codex resume <id>`、`claude --resume <id>`、
/// `opencode -s <id>`）。
pub(crate) fn resume_command(app: &str, id: &str) -> String {
    match app {
        "codex" => format!("codex resume {id}"),
        "claude" => format!("claude --resume {id}"),
        "opencode" => format!("opencode -s {id}"),
        _ => String::new(),
    }
}

/// 專案目錄的顯示名（`C:/Users/x/proj` → `proj`）。
#[cfg(test)]
pub(crate) fn project_basename(dir: &str) -> String {
    let d = dir.trim_end_matches(['/', '\\']);
    d.rsplit(['/', '\\']).next().unwrap_or(d).to_string()
}

/// 標題太長就截斷（清單要一行放得下）。
pub(crate) fn clean_title(s: &str) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = one_line.chars().take(120).collect();
    if t.is_empty() {
        "（無標題）".to_string()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_commands_match_the_real_clis() {
        // 這三條是照 `codex resume --help`、`claude --help`、`opencode --help` 寫的
        assert_eq!(resume_command("codex", "abc"), "codex resume abc");
        assert_eq!(resume_command("claude", "abc"), "claude --resume abc");
        assert_eq!(resume_command("opencode", "abc"), "opencode -s abc");
        assert_eq!(resume_command("gemini", "abc"), "", "不受管工具沒有指令");
    }

    #[test]
    fn project_basename_handles_both_separators() {
        assert_eq!(project_basename("C:/Users/x/proj"), "proj");
        assert_eq!(project_basename("C:\\Users\\x\\proj\\"), "proj");
        assert_eq!(project_basename("proj"), "proj");
        assert_eq!(project_basename(""), "");
    }

    #[test]
    fn titles_are_single_line_and_capped() {
        assert_eq!(clean_title("  多行\n標題  在這裡 "), "多行 標題 在這裡");
        assert_eq!(clean_title("   "), "（無標題）");
        assert_eq!(clean_title(&"字".repeat(200)).chars().count(), 120);
    }

    #[test]
    fn supported_apps_are_the_takeover_ones() {
        assert_eq!(supported_apps(), vec!["claude", "codex", "opencode"]);
    }
}
