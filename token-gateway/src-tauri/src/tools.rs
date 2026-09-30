//! M2：工具渠道檢測與切換適配。
//!
//! 安全約定：
//! - 金鑰一律走環境變量 [`GATEWAY_ENV_KEY`]（Codex `env_key` / OpenCode `{env:}` /
//!   終端機 export），絕不寫入各工具設定檔；唯一的例外是 Claude settings.json 的
//!   `env`（靜態值，無引用語法，只能寫明文——網關模式下請使用可吊銷的本地 sk key）。
//! - 每次寫入前自動備份到應用程式數據目錄 `backups/<app>/`，保留 [`BACKUP_KEEP`] 份。
//! - 所有檔案改寫都是純函數（輸入舊文本 → 輸出新文本），可單測；IO 只在
//!   [`apply_switch`] 一處發生。
//!
//! **支援範圍（2026-09-28 第一階段後）**：可接管／還原 = Claude Code、Codex、
//! OpenCode；只檢測不接管 = DSH；Cursor／Antigravity／Hermes 僅作為用量歸屬標籤。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod consts;
mod util;
mod detect;
mod versions;
mod apply;
mod codex;
mod switch;
mod backup;
mod native;
mod summary;

// 對外 API 再匯出（原本都在 `tools.rs` 頂層，`crate::tools::X` 路徑不變）。
//
// 子模組以 `use super::*;` 取用這裡的綁定 —— 這是必要的，因為 `use super::*`
// 只會帶進父模組**自己的**綁定，不會帶進父模組的 `use` 別名（那些是私有的），
// 也不會自動看到 `pub use`。
pub(crate) use {

    // ---- consts ----
    consts::APPS,
    consts::BACKUP_KEEP,
    consts::CODEX_ALIAS_CAP,
    consts::CODEX_INLINE_MODELS_CAP,
    consts::CODEX_SHARED_PROVIDER_ID,
    consts::GATEWAY_ENV_KEY,
    consts::GATEWAY_HOST,
    consts::GATEWAY_PROVIDER_ID,
    consts::TAKEOVER_APPS,

    // ---- util ----
    util::is_gateway,
    util::on_path,
    util::read_text,
    util::user_home,

    // ---- detect ----
    detect::ToolStatus,
    detect::detect_tools,

    // ---- versions ----
    versions::ToolLatest,
    versions::ToolVersion,
    versions::tool_latest,
    versions::tool_update_run,
    versions::tool_versions,

    // ---- apply ----
    apply::ClaudeModelMap,
    apply::claude_apply,
    apply::config_path_for,
    apply::opencode_apply,
    apply::ps_export,
    apply::tool_format,

    // ---- codex ----
    codex::CodexAuth,
    codex::LegacyProviders,
    codex::codex_alias_ids,
    codex::codex_apply,
    codex::codex_doctor,
    codex::codex_inline_models,
    codex::codex_legacy_providers_report,
    codex::codex_text_base_url,
    codex::CODEX_WIRE_API,
    codex::gateway_url,

    // ---- switch ----
    switch::SwitchPlan,
    switch::SwitchRequest,
    switch::SwitchResult,
    switch::apply_switch,
    switch::plan_switch,

    // ---- backup ----
    backup::ensure_baseline_before_prune,
    backup::prune_backups_keep_clean,
    backup::restore_backup_port,

    // ---- native ----
    // 只再匯出命令層真的用到的那個；`to_native` / `NativeOutcome` /
    // `CODEX_NATIVE_*` 由測試以 `crate::tools::native::…` 直接取用。
    native::restore_native_port,

    // ---- summary ----
    summary::restore_summary,
};

#[cfg(test)]
mod tests;
