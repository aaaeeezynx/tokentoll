//! 端到端：嚴格上游、剝離重試、診斷可見性
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。
//!
//! 模組佈局：本檔只留前置與子模組宣告；測試依主題放在
//! `e2e_strict/strip_retry.rs`（剝離後重發）、
//! `e2e_strict/strict_content.rs`（上游拒絕空 content）、
//! `e2e_strict/diagnostics.rs`（拒絕請求的診斷可見性）。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    #[path = "e2e_strict/strip_retry.rs"]
    mod strip_retry;
    #[path = "e2e_strict/strict_content.rs"]
    mod strict_content;
    #[path = "e2e_strict/diagnostics.rs"]
    mod diagnostics;
