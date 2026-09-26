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

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct QuotaWindow {
    /// 窗口顯示名（如「5 小時」「本週」「本月」）。
    pub label: String,
    /// 已用百分比 0-100。
    pub percent: f64,
    /// 重置時間（RFC3339；上游未給或零用量佔位時為 None）。
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionQuota {
    pub key: String,
    pub title: String,
    /// opencode-go | claude | codex
    pub kind: String,
    /// ok | no_credentials | expired | error | not_subscribed
    pub status: String,
    pub message: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub queried_at: i64,
}

impl SubscriptionQuota {
    fn ok(key: &str, title: &str, kind: &str, windows: Vec<QuotaWindow>) -> Self {
        Self {
            key: key.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            status: "ok".to_string(),
            message: None,
            windows,
            queried_at: crate::fsutil::now_ms(),
        }
    }

    fn fail(key: &str, title: &str, kind: &str, status: &str, message: impl Into<String>) -> Self {
        Self {
            key: key.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            status: status.to_string(),
            message: Some(message.into()),
            windows: vec![],
            queried_at: crate::fsutil::now_ms(),
        }
    }

    fn no_credentials(key: &str, title: &str, kind: &str, message: impl Into<String>) -> Self {
        Self::fail(key, title, kind, "no_credentials", message)
    }
}

fn clamp_percent(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 100.0)
}

fn unix_to_rfc3339(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339())
}

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

fn query_opencode_go(provider_id: i64, name: &str, base_url: &str, api_key: &str) -> SubscriptionQuota {
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

// ---------------------------------------------------------------- Claude ---

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

fn query_claude() -> SubscriptionQuota {
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

// ------------------------------------------------------------ Codex/ChatGPT ---

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

fn query_codex() -> SubscriptionQuota {
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

// ------------------------------------------------------------------ 入口 ---

/// 查詢所有可識別的訂閱額度：opencode-go 來源（逐個）+ Claude CLI + Codex CLI。
/// `providers` 為 (id, name, base_url, api_key, enabled)。
pub(crate) fn query_all(
    providers: &[(i64, String, String, String, bool)],
) -> Vec<SubscriptionQuota> {
    let mut out = vec![];
    for (id, name, base, key, enabled) in providers {
        if *enabled && detect_opencode_go(base) {
            out.push(query_opencode_go(*id, name, base, key));
        }
    }
    out.push(query_claude());
    out.push(query_codex());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detect_opencode_go_matches() {
        assert!(detect_opencode_go("https://opencode.ai/zen/go/v1"));
        assert!(detect_opencode_go("https://opencode.ai/zen/go"));
        assert!(!detect_opencode_go("https://opencode.ai/zen/v1"));
        assert!(!detect_opencode_go("https://api.openai.com/v1"));
    }

    #[test]
    fn opencode_windows_parsed_and_zero_reset_dropped() {
        let body = json!({"usage": {
            "rolling": {"status": "ok", "percent": 12.5, "resetsAt": "2026-08-13T16:27:38.287Z"},
            "weekly": {"status": "ok", "percent": 0, "resetsAt": "2026-08-17T00:00:00.287Z"},
            "monthly": {"status": "ok", "percent": 3, "resetsAt": "2026-09-13T06:06:01.287Z"}
        }});
        let w = parse_opencode_windows(&body);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].label, "5 小時");
        assert_eq!(w[0].percent, 12.5);
        assert!(w[0].resets_at.is_some());
        assert_eq!(w[1].percent, 0.0);
        assert!(w[1].resets_at.is_none(), "零用量窗口丟棄佔位重置時間");
        assert_eq!(w[2].label, "本月");
    }

    #[test]
    fn opencode_windows_skip_bad_status_and_clamp() {
        let body = json!({"usage": {
            "rolling": {"status": "error", "percent": 5},
            "weekly": {"status": "ok", "percent": 130},
            "monthly": {}
        }});
        let w = parse_opencode_windows(&body);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].percent, 100.0);
    }

    #[test]
    fn claude_windows_parsed_with_labels() {
        let body = json!({
            "five_hour": {"utilization": 4.0, "resets_at": "2026-08-13T20:00:00Z"},
            "seven_day": {"utilization": 61.2, "resets_at": "2026-08-17T00:00:00Z"},
            "extra_usage": {"is_enabled": false}
        });
        let w = parse_claude_windows(&body);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "5 小時");
        assert_eq!(w[1].label, "7 天");
        assert_eq!(w[1].percent, 61.2);
        assert_eq!(w[1].resets_at.as_deref(), Some("2026-08-17T00:00:00Z"));
    }

    #[test]
    fn codex_windows_parsed_with_seconds_label() {
        let body = json!({"rate_limit": {
            "primary_window": {"used_percent": 18, "limit_window_seconds": 18000, "reset_at": 1786000000},
            "secondary_window": {"used_percent": 42.5, "limit_window_seconds": 604800, "reset_at": 1786500000}
        }});
        let w = parse_codex_windows(&body);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "5 小時");
        assert_eq!(w[1].label, "7 天");
        assert_eq!(w[1].percent, 42.5);
        assert!(w[1].resets_at.as_deref().unwrap().starts_with("2026-"));
    }

    #[test]
    fn codex_window_label_fallbacks() {
        assert_eq!(codex_window_label(18000), "5 小時");
        assert_eq!(codex_window_label(2592000), "30 天");
        assert_eq!(codex_window_label(172800), "2 天");
        assert_eq!(codex_window_label(7200), "2 小時");
    }

    #[test]
    fn query_all_skips_disabled_and_non_go() {
        let providers = vec![
            (1, "go".into(), "https://opencode.ai/zen/go/v1".into(), "".into(), false),
            (2, "nim".into(), "https://integrate.api.nvidia.com/v1".into(), "k".into(), true),
        ];
        let out = query_all(&providers);
        // 停用的 Go 來源被跳過；claude/codex 兩條本地憑證查詢始終在列
        assert!(!out.iter().any(|q| q.kind == "opencode-go"));
        assert!(out.iter().any(|q| q.kind == "claude"));
        assert!(out.iter().any(|q| q.kind == "codex"));
    }
}
