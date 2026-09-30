//! 技能管理（P3.3，對標 cc-switch 的 Skills Management）。
//!
//! ## cc-switch 的模型（照它手冊 §3.3 抄）
//!
//! - 技能＝**一個資料夾**（裡面有 `SKILL.md`、prompt、工具定義、範例程式）。
//! - 有**母本儲存目錄**（預設 `~/.cc-switch/skills/`，可切到 `~/.agents/skills`），
//!   再**同步到各工具的 skills 目錄**（symlink 優先、失敗則複製）。
//! - 各工具目錄：Claude `~/.claude/skills/`、Codex `~/.codex/skills/`、
//!   OpenCode `~/.config/opencode/skills/`。
//! - 來源是 **GitHub 儲存庫**（owner／name／branch／subdir，可自訂、有內建），
//!   以及 skills.sh 公開登錄搜尋。
//! - **更新偵測用內容雜湊**（SHA-256 比對遠端與本機），支援單獨更新與全部更新。
//! - 解除安裝前**自動備份**到 `skill-backups/`，可從備份還原、也可刪除備份。
//!
//! ## 我們的版本
//!
//! 只服務**能接管的三個工具**（共用 `tools::TAKEOVER_APPS`）——與 MCP／提示詞同一條規矩。
//!
//! 差異（刻意，且寫在對齊矩陣）：
//! - **不做 skills.sh 搜尋**：那是第三方的公開登錄，我們沒辦法確認它的 API 與資料品質；
//!   使用者要的技能可以直接加 GitHub 儲存庫（cc-switch 也是靠儲存庫座標在裝）。
//! - **不做 ZIP 安裝**：離線快取裡沒有 zip 解析套件，而 GitHub tarball 已涵蓋主要路徑。
//! - **儲存位置切換**目前支援內建目錄（`<app_data>/skills/`）與 `~/.agents/skills`
//!   兩種，透過設定鍵切換（切換時會搬移既有技能）。

pub(crate) mod github;
pub(crate) mod install;
pub(crate) mod store;
pub(crate) mod sync;
mod tarball;

pub use install::{list_backups, uninstall_skill, BackupEntry, InstallOutcome};
pub use store::{add_repo, delete_repo, list_repos, list_skills, set_binding};
pub use sync::{sync_skill, SyncReport};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::tools::TAKEOVER_APPS;

/// 一個技能來源儲存庫。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SkillRepo {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub branch: String,
    /// 技能所在的子目錄（空＝儲存庫根目錄）
    pub subdir: String,
    pub label: String,
    /// 內建（不可刪除）
    pub builtin: bool,
}

/// 遠端掃到的一個技能（尚未安裝）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RemoteSkill {
    pub name: String,
    pub path: String,
    pub repo_id: i64,
    pub repo_label: String,
    pub installed: bool,
    /// 已安裝且遠端有變動 → 可以更新
    pub update_available: bool,
}

/// 已安裝的技能。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InstalledSkill {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub repo_id: Option<i64>,
    pub repo_label: String,
    pub remote_path: String,
    pub content_hash: String,
    pub installed_at: i64,
    pub updated_at: i64,
    /// 母本在儲存目錄裡的實際大小（bytes）
    pub size: u64,
    /// 逐工具啟用狀態
    pub bindings: Vec<SkillBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillBinding {
    pub app: String,
    pub enabled: bool,
}

/// 技能名稱的驗證：它會變成資料夾名稱，不允許路徑分隔與奇怪的東西。
pub(crate) fn validate_skill_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("技能名稱不能為空".to_string());
    }
    if n.len() > 64 {
        return Err("技能名稱太長（上限 64 字）".to_string());
    }
    let ok = n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok || n.starts_with('.') {
        return Err(format!(
            "技能名稱只能是英數與 - _ .（且不能以 . 開頭）：{n}"
        ));
    }
    Ok(n.to_string())
}

/// 儲存庫座標的驗證（owner／name 是 GitHub 的識別字）。
pub(crate) fn validate_repo(
    owner: &str,
    name: &str,
    branch: &str,
    subdir: &str,
) -> Result<(String, String, String, String), String> {
    let clean = |s: &str| s.trim().trim_matches('/').to_string();
    let (o, n, b, d) = (
        clean(owner),
        clean(name),
        if branch.trim().is_empty() {
            "main".to_string()
        } else {
            clean(branch)
        },
        clean(subdir),
    );
    let ok_seg = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !ok_seg(&o) || !ok_seg(&n) {
        return Err("owner 與 name 只能是英數與 - _ .".to_string());
    }
    if !ok_seg(&b) {
        return Err("branch 只能是英數與 - _ .".to_string());
    }
    // subdir 允許多層，但每一段都要合法
    for seg in d.split('/').filter(|s| !s.is_empty()) {
        if !ok_seg(seg) {
            return Err(format!("子目錄「{seg}」不合法"));
        }
    }
    Ok((o, n, b, d))
}

/// 內建儲存庫（**已實測存在**，見 §0.9.25）。
///
/// 只放確認過座標與版面配置的：`anthropics/skills` 的 `skills/` 底下是一個個
/// 技能資料夾（`academy-guide`、`claude-api`…），每個裡面都有 `SKILL.md`。
pub(crate) const BUILTIN_REPOS: &[(&str, &str, &str, &str, &str)] = &[(
    "anthropics",
    "skills",
    "main",
    "skills",
    "Anthropic 官方技能",
)];

/// 這個工具有沒有支援技能（我們只支援可接管的三個）。
#[cfg(test)]
pub fn supported_apps() -> Vec<&'static str> {
    TAKEOVER_APPS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_name_validation() {
        for ok in ["demo", "my-skill", "a_b.c", "Skill1"] {
            assert!(validate_skill_name(ok).is_ok(), "{ok} 應該可以");
        }
        for bad in ["", "  ", "a/b", "a\\b", ".hidden", "有中文", "a b"] {
            assert!(validate_skill_name(bad).is_err(), "{bad:?} 應該被拒");
        }
        assert!(validate_skill_name(&"x".repeat(65)).is_err());
        assert_eq!(validate_skill_name("  pad  ").unwrap(), "pad");
    }

    #[test]
    fn repo_validation_normalizes_and_rejects() {
        let (o, n, b, d) =
            validate_repo(" anthropics ", "/skills/", "", " skills/extra ", ).unwrap();
        assert_eq!((o.as_str(), n.as_str(), b.as_str(), d.as_str()),
                   ("anthropics", "skills", "main", "skills/extra"));
        assert!(validate_repo("", "x", "main", "").is_err());
        assert!(validate_repo("a", "b c", "main", "").is_err());
        assert!(validate_repo("a", "b", "main", "../etc").is_err());
    }

    /// 內建儲存庫必須通過我們自己的驗證（不然使用者一按就報錯）。
    #[test]
    fn builtin_repos_are_valid() {
        assert!(!BUILTIN_REPOS.is_empty());
        for (owner, name, branch, subdir, label) in BUILTIN_REPOS {
            assert!(
                validate_repo(owner, name, branch, subdir).is_ok(),
                "{owner}/{name} 不合法"
            );
            assert!(!label.is_empty(), "{owner}/{name} 要有標籤");
        }
    }

    #[test]
    fn supported_apps_are_the_takeover_ones() {
        assert_eq!(supported_apps(), vec!["claude", "codex", "opencode"]);
    }
}
