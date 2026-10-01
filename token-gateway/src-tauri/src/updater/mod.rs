//! 自動更新（P4.9 / D7）。
//!
//! ## cc-switch 的實際行為（照它手冊 §1.5 的 About 頁抄）
//!
//! 版本資訊、release notes、**檢查更新**、下載並安裝新版本。
//!
//! ## 我們做到哪裡（誠實說明）
//!
//! **做到「檢查更新」**：抓一份 Tauri v2 靜態更新 manifest（JSON），比對版本、
//! 顯示 release notes 與發布日期，並給出下載連結（用既有的 opener 外掛開啟）。
//!
//! **不做自動安裝**：Tauri 的自動安裝需要「簽章過的更新檔 ＋ 對應的公鑰」，
//! 而 `tauri-plugin-updater` 不在這個離線環境的 crate 快取裡、也沒有發佈管道。
//! 與其塞一個不能驗證的殼子，不如把「檢查」做完整、把「安裝」明確留給使用者
//! （對齊矩陣上標明）。你哪天架好 releases 頁與簽章，接上外掛就能升級成全自動。
//!
//! ## manifest 格式（Tauri v2 靜態 JSON）
//!
//! ```json
//! {
//!   "version": "0.2.0",
//!   "notes": "修正了…",
//!   "pub_date": "2026-01-01T00:00:00Z",
//!   "platforms": {
//!     "windows-x86_64": { "signature": "…", "url": "https://…/setup.exe" }
//!   }
//! }
//! ```

pub(crate) mod check;
#[cfg(test)]
mod tests;

// 命令層與測試各自直接用 `updater::check::*`，這裡只轉出「型別」給外部。
pub use check::{check_text, UpdateCheck};

/// manifest 的網址設定鍵（設定頁可填；留空＝不檢查）。
pub(crate) const URL_KEY: &str = "update_manifest_url";
/// 上次檢查時間與結果（設定頁顯示「上次檢查」）。
pub(crate) const LAST_KEY: &str = "update_last_check_ms";
pub(crate) const LAST_RESULT_KEY: &str = "update_last_result";
