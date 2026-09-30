//! 提示詞預設集（P3.2，對標 cc-switch 的 Prompts Management）。
//!
//! ## cc-switch 的模型（照它手冊 §3.2 抄）
//!
//! - **逐工具分開管理**（「Prompts are managed separately per app」）：切到 Claude
//!   就看到 Claude 的預設集，切到 Codex 就看到 Codex 的。跨工具要各建一份。
//! - 每個預設集＝名稱 ＋ Markdown 內容；面板上有 Markdown 編輯器。
//! - **同時只能有一個啟用**：啟用新的會自動把舊的關掉。
//! - 啟用後內容寫進該工具的檔案：
//!   Claude `~/.claude/CLAUDE.md`、Codex `~/.codex/AGENTS.md`、
//!   OpenCode `~/.config/opencode/AGENTS.md`。
//! - **智慧回填（smart backfill）**：切換預設集之前先讀現行檔案，如果內容與資料庫
//!   裡那個預設集不同（代表使用者手改過），就**先把檔案內容存回舊的預設集**再切。
//!   觸發時機：切換預設集時、編輯現行預設集時、首次啟動時匯入現有檔案。
//!   沒有啟用中的預設集時不回填。
//! - **啟用中的預設集不能刪**，要先關掉。
//!
//! ## 我們的範圍
//!
//! 與 MCP 同一條規矩：只服務**能接管的三個工具**（共用 `tools::TAKEOVER_APPS`），
//! 因為那三個的設定檔我們才有把握寫對。

mod store;
mod sync;

pub use store::{
    delete as delete_preset, get as get_preset, list as list_presets, save as save_preset,
};
pub(crate) use store::deactivate as deactivate_active;
pub use sync::{
    activate_and_sync, app_installed, ensure_imported, live_content, panel_state, path_for,
    supported_apps, sync_app, write_active, BackfillOutcome, PanelState,
};

use serde::{Deserialize, Serialize};

use crate::tools::TAKEOVER_APPS;

/// 內容長度上限（位元組）。cc-switch 只對 MiniMax Code 有 32 KiB 限制，
/// 我們的三個工具沒有硬限制 —— 這裡給一個寬鬆的自我保護上限。
pub const MAX_CONTENT_BYTES: usize = 512 * 1024;

/// 一個提示詞預設集。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PromptPreset {
    pub id: i64,
    /// claude／codex／opencode
    pub app: String,
    pub name: String,
    pub content: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 新增／編輯的輸入。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptInput {
    /// 編輯時帶 id；新增時 None
    pub id: Option<i64>,
    pub app: String,
    pub name: String,
    pub content: String,
}

impl PromptInput {
    /// 驗證並正規化。
    pub fn validated(&self) -> Result<PromptInput, String> {
        if !TAKEOVER_APPS.contains(&self.app.as_str()) {
            return Err(format!(
                "不支援的工具「{}」（提示詞只管理 {}）",
                self.app,
                TAKEOVER_APPS.join("／")
            ));
        }
        let name = self.name.trim();
        if name.is_empty() {
            return Err("預設集名稱不能為空".to_string());
        }
        if name.chars().count() > 80 {
            return Err("預設集名稱太長（上限 80 字）".to_string());
        }
        if self.content.len() > MAX_CONTENT_BYTES {
            return Err(format!(
                "內容太大（{} bytes，上限 {MAX_CONTENT_BYTES}）",
                self.content.len()
            ));
        }
        Ok(PromptInput {
            id: self.id,
            app: self.app.clone(),
            name: name.to_string(),
            content: self.content.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(app: &str, name: &str) -> PromptInput {
        PromptInput {
            id: None,
            app: app.into(),
            name: name.into(),
            content: "# 提示\n內容".into(),
        }
    }

    #[test]
    fn validation_accepts_the_three_apps() {
        for app in TAKEOVER_APPS {
            assert!(input(app, "名稱").validated().is_ok(), "{app} 應該可以");
        }
    }

    #[test]
    fn validation_rejects_other_apps_and_bad_names() {
        for app in ["hermes", "cursor", "gemini", "dsh", ""] {
            let e = input(app, "名稱").validated().unwrap_err();
            assert!(e.contains("不支援的工具"), "{app}：{e}");
        }
        assert!(input("codex", "   ")
            .validated()
            .unwrap_err()
            .contains("名稱不能為空"));
        assert!(input("codex", &"字".repeat(81))
            .validated()
            .unwrap_err()
            .contains("太長"));
        // 名稱會去空白
        assert_eq!(input("codex", "  我的提示  ").validated().unwrap().name, "我的提示");
    }

    #[test]
    fn validation_caps_content_size() {
        let mut i = input("codex", "大檔");
        i.content = "x".repeat(MAX_CONTENT_BYTES + 1);
        assert!(i.validated().unwrap_err().contains("內容太大"));
        // 剛好到上限可以
        i.content = "x".repeat(MAX_CONTENT_BYTES);
        assert!(i.validated().is_ok());
    }
}
