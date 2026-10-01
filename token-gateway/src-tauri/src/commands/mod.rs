//! Tauri 命令層（M1）：只讀狀態查詢。M2 起陸續接入增刪改查。
//!
//! 本模組原本是單一檔案（1,233 行），現依主題拆成下列子模組；此處只保留
//! 子模組宣告與再匯出。所有項目一律以 `pub use <子模組>::*;` 重新匯出，
//! 因此 `crate::commands::X` 這個路徑對每個原名稱都不變 —— 包含 `lib.rs`
//! 的 `generate_handler![commands::X, ...]`：`#[tauri::command]` 產生的
//! `__cmd__X`／`__tauri_command_name_X` 巨集同樣經由這條 glob 再匯出而可見。
//!
//! 子模組分佈：
//! - `status`：DB／網關狀態與啟停
//! - `settings`：應用程式設定
//! - `local_keys`：本地 API Key
//! - `usage_cmds`：用量查詢、報價與 range 解析共用純函式
//! - `pricing_cmds`：全域與渠道定價
//! - `logs_history`：請求日誌、歷史回填與資料目錄
//! - `apps`：各本機工具的接管／還原
//! - `models_cmds`：模型目錄與價格識別
//! - `quota_cmd`：訂閱額度
//! - `usage_query_cmd`：每來源的用量查詢設定與查詢（P2.1）
//! - `diagnostics`：診斷中心
//! - `tests`：純函式單測（僅測試組建）

mod apps;
mod diagnostics;
mod local_keys;
mod logs_history;
mod models_cmds;
mod pricing_cmds;
mod quota_cmd;
mod appearance_cmd;
mod cloud_cmd;
mod dbbackup_cmd;
mod deeplink_cmd;
mod updater_cmd;
mod window_cmd;
mod settings;
mod status;
mod usage_cmds;
mod usage_query_cmd;

#[cfg(test)]
mod tests;

pub use apps::*;
pub use diagnostics::*;
pub use local_keys::*;
pub use logs_history::*;
pub use models_cmds::*;
pub use pricing_cmds::*;
pub use quota_cmd::*;
pub use appearance_cmd::*;
pub use cloud_cmd::*;
pub use dbbackup_cmd::*;
pub use deeplink_cmd::*;
pub use updater_cmd::*;
pub use window_cmd::*;
pub use settings::*;
pub use status::*;
pub use usage_cmds::*;
pub use usage_query_cmd::*;
