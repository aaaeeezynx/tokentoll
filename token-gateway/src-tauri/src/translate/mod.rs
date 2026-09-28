//! M7：Anthropic Messages ⇄ OpenAI Chat Completions 翻譯。
//!
//! 只做 **Anthropic 進 → OpenAI 出**（Claude Code 經網關打 OpenAI 兼容上游，
//! 即用戶現有的 cc-switch 流程）。反向遇到時回明確 400，不靜默轉發。
//!
//! 模型名映射走可用模型表（display_name → actual_model），見 `models::resolve_actual`。
//!
//! 本模組按方向／主題拆成子模組：`openai_chat`（Anthropic 進 → OpenAI 出的請求側）、
//! `sanitize`（Chat 消息配對修復）、`anthropic`（OpenAI 回 → Anthropic，含流式）、
//! `responses`＋`responses_tools`＋`responses_stream`＋`responses_finish`
//!（Responses ⇄ Chat Completions）。子模組的項目在此以 `pub(crate) use` 重新導出，
//! 因此 `crate::translate::X` 對每個原有名字都不變。

use serde_json::Value;

/// 流式/非流式共用的用量累積（代理記帳用）。
#[derive(Debug, Default, Clone)]
pub(crate) struct SseUsage {
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub model: Option<String>,
}

impl SseUsage {
    pub fn total(&self) -> i64 {
        self.in_tok + self.out_tok + self.cache_read + self.cache_write
    }
}

pub(crate) fn num(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key)?.as_i64()
}

/// OpenAI usage 對象 → 累積（chat / responses 通用）。
pub(crate) fn read_openai_usage(u: &serde_json::Value, acc: &mut SseUsage) {
    if let Some(n) = num(u, "prompt_tokens").or(num(u, "input_tokens")) {
        acc.in_tok = n;
    }
    if let Some(n) = num(u, "completion_tokens").or(num(u, "output_tokens")) {
        acc.out_tok = n;
    }
    if let Some(n) = num(u, "cache_read_input_tokens") {
        acc.cache_read = n;
    }
    if let Some(n) = num(u, "cache_creation_input_tokens") {
        acc.cache_write = n;
    }
    if let Some(det) = u.get("prompt_tokens_details") {
        if let Some(n) = num(det, "cached_tokens") {
            acc.cache_read = n;
        }
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// 缺失的歷史 tool 輸出佔位（舊 session 續跑 / 中斷輪次常見）：讓上游配對校驗通過，
/// 模型按現有上下文繼續，不會把佔位當真實執行結果。
const MISSING_TOOL_OUTPUT: &str =
    "[Previous tool result unavailable — restored from session history; continue with available context.]";

/// 非 assistant 空消息佔位：部分嚴格上游（如 OpenRouter 後端 ModelRun）要求
/// 非 assistant 角色的 content 非空，空字串/缺失會 `message content is required
/// for non-assistant roles` 400（2026-09-24 codex 實例）。assistant 豁免
///（帶 tool_calls 的 content:null 與純空文本均為 OpenAI 合法形狀）。
const EMPTY_MESSAGE_PLACEHOLDER: &str = "[empty message — placeholder filled by gateway]";

mod anthropic;
mod openai_chat;
mod responses;
mod responses_finish;
mod responses_stream;
mod responses_tools;
mod sanitize;

pub(crate) use anthropic::*;
pub(crate) use openai_chat::*;
pub(crate) use responses::*;
pub(crate) use responses_finish::*;
pub(crate) use responses_stream::*;
pub(crate) use responses_tools::*;
pub(crate) use sanitize::*;

#[cfg(test)]
mod tests;
