//! Claude 訂閱額度：讀 `~/.claude/.credentials.json` 的 OAuth token 查 oauth usage。

use serde_json::Value;

use super::clamp_percent;
use super::types::{QuotaWindow, SubscriptionQuota};

fn tier_label(name: &str) -> String {
    match name {
        "five_hour" => "5 小時".to_string(),
        "seven_day" => "7 天".to_string(),
        "seven_day_opus" => "7 天（Opus）".to_string(),
        "seven_day_sonnet" => "7 天（Sonnet）".to_string(),
        "30_day" => "30 天".to_string(),
        other => other.to_string(),
    }
}

/// 解析 Claude oauth usage 回包：各窗口 `utilization` + `resets_at`（ISO），
/// `extra_usage` 等非窗口字段跳過。
pub(crate) fn parse_claude_windows(body: &Value) -> Vec<QuotaWindow> {
    let mut out = vec![];
    let Some(obj) = body.as_object() else {
        return out;
    };
    for (key, val) in obj {
        if key == "extra_usage" {
            continue;
        }
        let Some(util) = val.get("utilization").and_then(|u| u.as_f64()) else {
            continue;
        };
        let resets_at = val
            .get("resets_at")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        out.push(QuotaWindow {
            label: tier_label(key),
            percent: clamp_percent(util),
            resets_at,
        });
    }
    out
}

pub(crate) fn query_claude() -> SubscriptionQuota {
    let key_s = "claude-cli";
    let title = "Claude 訂閱（CLI 憑證）";
    let token = match read_claude_token() {
        Ok(Some(t)) => t,
        Ok(None) => {
            return SubscriptionQuota::no_credentials(
                key_s,
                title,
                "claude",
                "未找到 ~/.claude/.credentials.json：請先用 Claude CLI 登入",
            )
        }
        Err(e) => return SubscriptionQuota::fail(key_s, title, "claude", "error", e),
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("token-gateway/0.1 quota")
        .build()
    {
        Ok(c) => c,
        Err(e) => return SubscriptionQuota::fail(key_s, title, "claude", "error", e.to_string()),
    };
    let resp = match client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("accept", "application/json")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return SubscriptionQuota::fail(key_s, title, "claude", "error", format!("連接失敗：{e}"))
        }
    };
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return SubscriptionQuota::fail(
            key_s,
            title,
            "claude",
            "expired",
            format!("憑證失效（HTTP {status}），請重新登入 Claude CLI"),
        );
    }
    if !status.is_success() {
        let snippet: String = resp.text().unwrap_or_default().chars().take(200).collect();
        return SubscriptionQuota::fail(
            key_s,
            title,
            "claude",
            "error",
            format!("HTTP {status}：{}", snippet.trim()),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return SubscriptionQuota::fail(
                key_s,
                title,
                "claude",
                "error",
                format!("解析回包失敗：{e}"),
            )
        }
    };
    let windows = parse_claude_windows(&body);
    if windows.is_empty() {
        return SubscriptionQuota::fail(key_s, title, "claude", "error", "回包沒有可識別的用量窗口");
    }
    SubscriptionQuota::ok(key_s, title, "claude", windows)
}

fn read_claude_token() -> Result<Option<String>, String> {
    let home = crate::tools::user_home()?;
    let path = home.join(".claude").join(".credentials.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("讀取憑證失敗：{e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("憑證不是合法 JSON：{e}"))?;
    let token = v
        .get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() {
        return Ok(None);
    }
    Ok(Some(token))
}
