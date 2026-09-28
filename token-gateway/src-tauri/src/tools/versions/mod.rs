//! 版本探測與更新
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。
//!
//! 模組佈局：`probe` 放版本探測與進程小工具；`update` 放一鍵更新與 registry 查詢；
//! `version_cmp` 放版本字串比較；兩個子模組共用的 npm 包名與 PATH 解析留在本檔；
//! 對外路徑 `crate::tools::versions::*` 由下方 re-export 保持不變。

use super::*;

mod probe;
mod update;
mod version_cmp;

// 保持原本 `crate::tools::versions::*` 的取名路徑不變（部分條目僅測試使用，故允許未使用)。
#[allow(unused_imports)]
pub(crate) use probe::*;
#[allow(unused_imports)]
pub(crate) use update::*;
#[allow(unused_imports)]
pub(crate) use version_cmp::*;

/// 本機工具版本 + 一鍵更新資訊（版本頁用；與 10 秒輪詢的 detect 分開，避免頻繁起進程）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolVersion {
    pub app: String,
    pub display: String,
    pub installed: bool,
    /// 版本字串（取首行），未安裝或取不到時為 None。
    pub version: Option<String>,
    /// 更新命令 argv（不經 shell，直接執行；為空 = 不支援一鍵更新）。
    pub update_argv: Vec<String>,
    /// 更新命令的人類可讀展示。
    pub update_label: Option<String>,
    /// 是否已是最新（由 `tool_latest` 填；`tool_versions` 一律留 None）。
    pub up_to_date: Option<bool>,
}


/// 最新版查詢結果（走 registry，有網絡延遲；前端獨立查詢，不擋版本顯示）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolLatest {
    pub app: String,
    pub latest: Option<String>,
    pub up_to_date: Option<bool>,
}


/// npm 全局包名（claude/codex/opencode/dsh 走 npm 更新）。
pub(crate) fn npm_package(app: &str) -> Option<&'static str> {
    match app {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "opencode" => Some("opencode-ai"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}


/// 在 PATH 中解析可執行檔完整路徑（Windows 含常見副檔名）。
pub(crate) fn which_bin(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for d in std::env::split_paths(&path) {
        for n in names {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
            #[cfg(windows)]
            for ext in ["exe", "cmd", "ps1"] {
                let q = d.join(format!("{n}.{ext}"));
                if q.is_file() {
                    return Some(q);
                }
            }
        }
    }
    None
}
