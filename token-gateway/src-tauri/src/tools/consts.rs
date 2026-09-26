//! 常數與應用清單
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。



pub const GATEWAY_HOST: &str = "127.0.0.1";

/// 各工具引用本地 Key 的統一環境變量名。
pub const GATEWAY_ENV_KEY: &str = "TOKEN_GATEWAY_KEY";

/// 我們在各工具配置裡創建的渠道 ID。
pub const GATEWAY_PROVIDER_ID: &str = "tokengateway";

/// Codex 共享段名（cc-switch 同款）：會話歷史按段名引用供應商，段名必須
/// 永遠存在且指向網關，舊會話才不會因段被刪而「Model provider not found」。
pub const CODEX_SHARED_PROVIDER_ID: &str = "custom";

/// Hermes 自定義渠道按其自有約定讀取的 Key 名：`HERMES_CUSTOM_<PROVIDER>_API_KEY`。
pub const HERMES_CUSTOM_ENV_KEY: &str = "HERMES_CUSTOM_TOKENGATEWAY_API_KEY";

pub const BACKUP_KEEP: usize = 10;


/// (app_id, 顯示名稱)
pub const APPS: [(&str, &str); 7] = [
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("opencode", "OpenCode"),
    ("hermes", "Hermes Agent"),
    ("dsh", "DeepSeek Harness"),
    ("cursor", "Cursor"),
    ("antigravity", "Antigravity"),
];


/// 歷史別名上限（threads 去重後的 provider 名，防止失控膨脹）。
pub(crate) const CODEX_ALIAS_CAP: usize = 32;

/// provider 段內聯 models 上限（桌面版選擇器顯示用）。
pub(crate) const CODEX_INLINE_MODELS_CAP: usize = 200;
