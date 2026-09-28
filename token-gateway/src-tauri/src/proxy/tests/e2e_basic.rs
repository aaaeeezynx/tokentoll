//! 端到端：認證、記錄、模型守門、改道
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。
//!
//! 模組佈局：本檔只留前置與子模組宣告；測試依情境放在
//! `e2e_basic/forward.rs`（Google 金鑰轉發、非流式轉發＋鑑權＋日誌）、
//! `e2e_basic/routing.rs`（未登記模型守門、跨來源改道）、
//! `e2e_basic/passthrough.rs`（display→actual 改名、並行 tool_calls 合併）。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    #[path = "e2e_basic/forward.rs"]
    mod forward;
    #[path = "e2e_basic/routing.rs"]
    mod routing;
    #[path = "e2e_basic/passthrough.rs"]
    mod passthrough;
