//! ChatGPT（Codex OAuth）額度：讀 `~/.codex/auth.json` 查 wham/usage。

use serde_json::Value;

use super::clamp_percent;
use super::types::{QuotaWindow, SubscriptionQuota};
use super::unix_to_rfc3339;

pub(crate) fn codex_window_label(seconds: i64) -> String {
    match seconds {
        18_000 => "5 小時".to_string(),
        604_800 => "7 天".to_string(),
        2_592_000 => "30 天".to_string(),
        s if s > 0 && s % 86_400 == 0 => format!("{} 天", s / 86_400),
        s if s > 0 && s % 3_600 == 0 => format!("{} 小時", s / 3_600),
        s => format!("{s} 秒"),
    }
}

/// 解析 ChatGPT wham/usage 回包：primary/secondary 窗口。
pub(crate) fn parse_codex_windows(body: &Value) -> Vec<QuotaWindow> {
    let mut out = vec![];
    let Some(rl) = body.get("rate_limit") else {
        return out;
    };
    for slot in ["primary_window", "secondary_window"] {
        let Some(w) = rl.get(slot) else { continue };
        let Some(used) = w.get("used_percent").and_then(|u| u.as_f64()) else {
            continue;
        };
        let label = w
            .get("limit_window_seconds")
            .and_then(|s| s.as_i64())
            .map(codex_window_label)
            .unwrap_or_else(|| slot.to_string());
        let resets_at = w.get("reset_at").and_then(|r| r.as_i64()).and_then(unix_to_rfc3339);
        out.push(QuotaWindow {
            label,
            percent: clamp_percent(used),
            resets_at,
        });
    }
    out
}

pub(crate) fn query_codex() -> SubscriptionQuota {
    let key_s = "codex-cli";
    let title = "ChatGPT（Codex OAuth）";
    let (token, account_id) = match read_codex_credentials() {
        Ok(Some(t)) => t,
        Ok(None) => {
            return SubscriptionQuota::no_credentials(
                key_s,
                title,
                "codex",
                "未找到 ~/.codex/auth.json 的 ChatGPT 登入：請先 codex login（API Key 模式無訂閱額度）",
            )
        }
        Err(e) => return SubscriptionQuota::fail(key_s, title, "codex", "error", e),
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return SubscriptionQuota::fail(key_s, title, "codex", "error", e.to_string()),
    };
    let mut req = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .bearer_auth(&token)
        .header("user-agent", "codex-cli")
        .header("accept", "application/json");
    if let Some(id) = &account_id {
        req = req.header("ChatGPT-Account-Id", id);
    }
    let resp = match req.send() {
        Ok(r) => r,
        Err(e) => {
            return SubscriptionQuota::fail(key_s, title, "codex", "error", format!("連接失敗：{e}"))
        }
    };
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return SubscriptionQuota::fail(
            key_s,
            title,
            "codex",
            "expired",
            format!("憑證失效（HTTP {status}），請重新 codex login"),
        );
    }
    if !status.is_success() {
        let snippet: String = resp.text().unwrap_or_default().chars().take(200).collect();
        return SubscriptionQuota::fail(
            key_s,
            title,
            "codex",
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
                "codex",
                "error",
                format!("解析回包失敗：{e}"),
            )
        }
    };
    let windows = parse_codex_windows(&body);
    if windows.is_empty() {
        return SubscriptionQuota::fail(key_s, title, "codex", "error", "回包沒有可識別的用量窗口");
    }
    SubscriptionQuota::ok(key_s, title, "codex", windows)
}

fn read_codex_credentials() -> Result<Option<(String, Option<String>)>, String> {
    let home = crate::tools::user_home()?;
    let path = home.join(".codex").join("auth.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("讀取憑證失敗：{e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("憑證不是合法 JSON：{e}"))?;
    let auth_mode = v.get("auth_mode").and_then(|m| m.as_str()).unwrap_or("");
    if !auth_mode.is_empty() && auth_mode != "chatgpt" {
        return Ok(None);
    }
    let tokens = match v.get("tokens") {
        Some(t) => t,
        None => return Ok(None),
    };
    let token = tokens
        .get("access_token")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() {
        return Ok(None);
    }
    let account_id = tokens
        .get("account_id")
        .and_then(|a| a.as_str())
        .map(|s| s.to_string());
    Ok(Some((token, account_id)))
}
