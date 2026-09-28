//! 訂閱額度查詢：查「方案用量百分比 + 重置時間」（參考 cc-switch 做法）。
//!
//! - OpenCode Go：`GET {base_url}/usage`，Bearer 用 provider 的推理 Key，
//!   返回 rolling（5 小時）/ weekly（本週）/ monthly（本月）三個窗口的 percent 與 resetsAt。
//! - Claude 訂閱：讀 `~/.claude/.credentials.json` 的 OAuth token，
//!   查 `api.anthropic.com/api/oauth/usage`（utilization + resets_at）。
//! - ChatGPT（Codex OAuth）：讀 `~/.codex/auth.json` 的 access_token，
//!   查 `chatgpt.com/backend-api/wham/usage`（used_percent + limit_window_seconds + reset_at）。
//!
//! 只讀本地憑證、只調官方查詢接口；憑證與 token 絕不回傳前端。
//! 訂閱類「用量」語義就是額度百分比，不按 token 單價折算。
//!
//! 模組佈局：`types` 放資料結構；`opencode` / `claude` / `codex` 各放一家上游的查詢與解析；
//! 兩家以上共用的百分比夾取與時間轉換留在本檔；對外路徑 `crate::quota::*` 由下方 re-export 保持不變。

mod claude;
mod codex;
mod opencode;
mod types;

// 保持原本 `crate::quota::*` 的取名路徑不變（部分條目僅測試使用，故允許未使用)。
#[allow(unused_imports)]
pub(crate) use claude::*;
#[allow(unused_imports)]
pub(crate) use codex::*;
#[allow(unused_imports)]
pub(crate) use opencode::*;
#[allow(unused_imports)]
pub use types::*;

#[cfg(test)]
mod tests;

fn clamp_percent(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 100.0)
}

fn unix_to_rfc3339(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
}

// ------------------------------------------------------------------ 入口 ---

/// 查詢所有可識別的訂閱額度：opencode-go 來源（逐個）+ Claude CLI + Codex CLI。
/// `providers` 為 (id, name, base_url, api_key, enabled)。
pub(crate) fn query_all(
    providers: &[(i64, String, String, String, bool)],
) -> Vec<SubscriptionQuota> {
    let mut out = vec![];
    for (id, name, base, key, enabled) in providers {
        if *enabled && detect_opencode_go(base) {
            out.push(opencode::query_opencode_go(*id, name, base, key));
        }
    }
    out.push(claude::query_claude());
    out.push(codex::query_codex());
    out
}
