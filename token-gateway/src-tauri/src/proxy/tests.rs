//! 測試子模組的入口與共用前置。
//!
//! 原本所有測試都擠在這一個檔案（2,025 行），已依主題拆成六個子模組。
//! 這裡保留前置並以 `#[path]` 引入子模組，讓 **`proxy::tests::<name>` 這個
//! 路徑完全不變** —— 既有測試名稱、`cargo test` 的篩選字串、以及 `#[ignore]`
//! 的執行方式都不必改。
//!
//! 子模組住在更深的層級，`use super::*;` 只會帶進**父模組自己的**綁定，不會
//! 帶進父模組的 `use` 別名（別名是私有的）。所以子模組需要的型別與常數一律
//! 用 `pub(super) use` 再匯出 —— 同一個名字只寫一次，避免 E0252。

#![allow(unused_imports)]

use super::*;
use axum::http::HeaderValue;

// 子模組真正需要的東西，以 `pub(super)` 再匯出（見上方說明）：
// - `SseUsage`：原本經由 `proxy.rs` 的檔案層級 `use crate::translate::{…}` 取得，
//   該行隨 `extract_usage` 搬進 `proxy/logging.rs`，故在此明確引入。
// - `E_*`：矩陣的 400 訊息常數住在 `proxy::matrix`（第十一步拆出）。
pub(super) use crate::proxy::matrix::{
    E_ANTHROPIC_UNSUPPORTED, E_CHAT_TO_RESPONSES, E_GEMINI_IN_ONLY, E_GEMINI_OUT_ONLY,
    E_REVERSE_UNSUPPORTED,
};
pub(super) use crate::translate::SseUsage;

#[path = "tests/unit.rs"]
mod unit;
#[path = "tests/strip.rs"]
mod strip;
#[path = "tests/matrix.rs"]
mod matrix;
#[path = "tests/e2e_basic.rs"]
mod e2e_basic;
#[path = "tests/e2e_translate.rs"]
mod e2e_translate;
#[path = "tests/e2e_strict.rs"]
mod e2e_strict;
