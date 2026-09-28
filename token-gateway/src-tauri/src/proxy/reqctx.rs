//! 請求脈絡（`ReqCtx`）：一次請求中**每個階段都要用**的欄位。
//!
//! 背景：`RetryCtx` 與 `FinishCtx` 原本各自重複列出 `ctx`／`started`／`app`／
//! `model_raw` 這 4 個欄位，`PrepareInput` 又再列一次。`proxy_handler` 因此
//! 得在三處重複填同一組值（見 §3 的「行數高爾夫」註記）。
//!
//! 把共同欄位集中在這裡之後：
//! - `proxy_handler` 只建一次 `ReqCtx`
//! - `RetryCtx` / `FinishCtx` 各嵌入一個 `ReqCtx`（用 `..` 展開）
//! - 新增階段時不必再重複這 4 個欄位
//!
//! 這是**純結構重組**：欄位語意、生命週期、呼叫順序完全不變。

use super::*;

/// 一次請求的共用脈絡。全部是借用，`Copy` 成本極低，可自由傳遞。
#[derive(Clone, Copy)]
pub(super) struct ReqCtx<'a> {
    pub(super) ctx: &'a ProxyCtx,
    pub(super) started: &'a Instant,
    /// 來源 App（`infer_app` 判定，全請求只算一次）。
    pub(super) app: &'a str,
    /// 客戶端送來的原始模型名。
    pub(super) model_raw: &'a str,
}

impl<'a> ReqCtx<'a> {
    pub(super) fn new(
        ctx: &'a ProxyCtx,
        started: &'a Instant,
        app: &'a str,
        model_raw: &'a str,
    ) -> Self {
        Self {
            ctx,
            started,
            app,
            model_raw,
        }
    }
}
