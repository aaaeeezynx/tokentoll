//! Codex 接管（config.toml、別名、協議、歷史 provider）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。
//!
//! 模組佈局：本檔只留子模組宣告與再匯出；內容依主題放在
//! - `codex/legacy.rs`：TOML 重複段消毒、歷史 provider 別名與內嵌模型讀取
//! - `codex/wire.rs`：網關 URL、協議推定、`CodexAuth` 與 `codex_apply` 改寫
//! - `codex/doctor.rs`：接管前體檢
//!
//! 子模組以 `use super::super::*;` 取用父模組 `tools` 的共用前置（與原檔
//! `use super::*;` 等價），並以 `pub use <子模組>::*;` 再匯出，因此
//! `crate::tools::codex::X` 這個路徑對每個原名稱都不變。

mod doctor;
mod legacy;
mod wire;

pub use doctor::*;
pub use legacy::*;
pub use wire::*;
