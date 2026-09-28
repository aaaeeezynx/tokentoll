//! `crate::translate` 的單元測試，按主題分檔：`anthropic`（Anthropic ⇄ OpenAI）、
//! `responses`（Responses ⇄ Chat 的請求／回應與工具）、`sanitize`（消息配對修復）、
//! `streaming`（Responses 流式事件）。

mod anthropic;
mod responses;
mod sanitize;
mod streaming;
