//! OpenCode Go 方案額度：`GET {base_url}/usage`。

use serde_json::Value;

use super::clamp_percent;
use super::types::{QuotaWindow, SubscriptionQuota};

// ------------------------------------------------------------ OpenCode Go ---

pub(crate) fn detect_opencode_go(base_url: &str) -> bool {
    base_url.to_lowercase().contains("opencode.ai/zen/go")
}

/// 解析 `/usage` 回包：rolling/weekly/monthly 三窗口。
/// 零用量窗口丟棄佔位重置時間（上游會回一個無意義的 resetsAt）。
pub(crate) fn parse_opencode_windows(body: &Value) -> Vec<QuotaWindow> {
    let mut out = vec![];
    let Some(usage) = body.get("usage") else {
        return out;
    };
    for (key, label) in [
        ("rolling", "5 小時"),
        ("weekly", "本週"),
        ("monthly", "本月"),
    ] {
        let Some(w) = usage.get(key) else { continue };
        let Some(percent) = w.get("percent").and_then(|p| p.as_f64()) else {
            continue;
        };
        let status = w.get("status").and_then(|s| s.as_str()).unwrap_or("ok");
        if status != "ok" {
            continue;
        }
        let percent = clamp_percent(percent);
        let resets_at = if percent > 0.0 {
            w.get("resetsAt")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        out.push(QuotaWindow {
            label: label.to_string(),
            percent,
            resets_at,
        });
    }
    out
}

pub(crate) fn query_opencode_go(provider_id: i64, name: &str, base_url: &str, api_key: &str) -> SubscriptionQuota {
    let key = format!("opencode-go:{provider_id}");
    let title = format!("OpenCode Go（{name}）");
    if api_key.trim().is_empty() {
        return SubscriptionQuota::no_credentials(&key, &title, "opencode-go", "該來源未填寫 API Key");
    }
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/usage");
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("token-gateway/0.1 quota")
        .build()
    {
        Ok(c) => c,
        Err(e) => return SubscriptionQuota::fail(&key, &title, "opencode-go", "error", e.to_string()),
    };
    let resp = match client.get(&url).bearer_auth(api_key.trim()).send() {
        Ok(r) => r,
        Err(e) => {
            return SubscriptionQuota::fail(
                &key,
                &title,
                "opencode-go",
                "error",
                format!("連接失敗：{e}"),
            )
        }
    };
    let status = resp.status();
    if status.as_u16() == 403 {
        return SubscriptionQuota::fail(
            &key,
            &title,
            "opencode-go",
            "not_subscribed",
            "此 Key 未訂閱 OpenCode Go 方案（HTTP 403）",
        );
    }
    if status.as_u16() == 401 {
        return SubscriptionQuota::fail(
            &key,
            &title,
            "opencode-go",
            "expired",
            "Key 鑑權失敗（HTTP 401），請重新生成",
        );
    }
    if !status.is_success() {
        let snippet: String = resp.text().unwrap_or_default().chars().take(200).collect();
        return SubscriptionQuota::fail(
            &key,
            &title,
            "opencode-go",
            "error",
            format!("HTTP {status}：{}", snippet.trim()),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return SubscriptionQuota::fail(
                &key,
                &title,
                "opencode-go",
                "error",
                format!("解析回包失敗：{e}"),
            )
        }
    };
    let windows = parse_opencode_windows(&body);
    if windows.is_empty() {
        return SubscriptionQuota::fail(
            &key,
            &title,
            "opencode-go",
            "error",
            "回包沒有可識別的用量窗口",
        );
    }
    SubscriptionQuota::ok(&key, &title, "opencode-go", windows)
}
