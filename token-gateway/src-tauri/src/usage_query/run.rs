//! 真的去打一次用量查詢（P2.1 的「測試」與卡片查詢共用這一支）。
//!
//! 與 `provider_check.rs` 的差別：連線檢查只看位址可不可達（不帶金鑰），
//! 這裡**會帶上使用者設定的憑證**去打那個查詢端點 —— 因為它要拿回真正的數字。
//! cc-switch 也提醒「用量查詢會消耗少量 API 額度」，所以自動查詢間隔預設是 0（關）。

use std::time::Duration;

use super::{apply_extractor, expand, QuotaView, UsageQueryConfig};

/// 執行一次查詢。`api_key`／`base_url` 由呼叫端從來源帶進來（設定裡的空值＝沿用來源）。
pub(crate) async fn run(
    client: &reqwest::Client,
    cfg: &UsageQueryConfig,
    api_key: &str,
    base_url: &str,
) -> QuotaView {
    let url = expand(&cfg.url_template, cfg, api_key, base_url);
    if url.trim().is_empty() {
        return QuotaView::failed(cfg.provider_id, "查詢網址是空的（先用樣板或自己填）");
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return QuotaView::failed(
            cfg.provider_id,
            format!("查詢網址必須是 http(s)：{url}"),
        );
    }
    let timeout = Duration::from_secs(cfg.timeout_secs.clamp(1, 120) as u64);
    let method = cfg.method.trim().to_ascii_uppercase();
    let mut req = if method == "POST" {
        client.post(&url)
    } else {
        client.get(&url)
    }
    .timeout(timeout)
    .header("accept", "application/json")
    .header("user-agent", "tokentoll/usage-query");
    for h in &cfg.headers {
        let name = h.name.trim();
        if name.is_empty() {
            continue;
        }
        req = req.header(name, expand(&h.value, cfg, api_key, base_url));
    }
    match req.send().await {
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            if !status.is_success() {
                // 非 2xx：把狀態碼與回應片段一起回報（樣板錯通常是 404/401）
                let mut v = apply_extractor(&cfg.extractor, cfg.provider_id, &text);
                v.ok = false;
                v.message = format!(
                    "上游回 {}：{}",
                    status.as_u16(),
                    super::truncate(text.trim(), 200)
                );
                return v;
            }
            apply_extractor(&cfg.extractor, cfg.provider_id, &text)
        }
        Err(e) => {
            let hint = if e.is_timeout() {
                format!("查詢逾時（{} 秒）", cfg.timeout_secs)
            } else if e.is_connect() {
                "連不上查詢端點（檢查網址與網路）".to_string()
            } else {
                "查詢失敗".to_string()
            };
            QuotaView::failed(cfg.provider_id, format!("{hint}：{e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_query::HeaderPair;

    fn cfg(url: &str) -> UsageQueryConfig {
        UsageQueryConfig {
            provider_id: 9,
            enabled: true,
            url_template: url.to_string(),
            method: "GET".into(),
            // New API 形狀：quota / used_quota 都要除以 500000 換成 USD
            extractor: serde_json::json!({
                "remaining": {"path": "data.quota", "divide": 500000},
                "used": {"path": "data.used_quota", "divide": 500000},
                "total": {"path": "data.quota", "divide": 500000},
                "unit": "USD",
                "planName": "data.group",
                "validPath": "success",
                "invalidMessagePath": "message"
            }),
            timeout_secs: 5,
            ..Default::default()
        }
    }

    /// 真的打一個本機伺服器：驗證「佔位符 → 請求 → 抽取」整條路徑。
    #[tokio::test]
    async fn runs_against_a_real_endpoint() {
        let app = axum::Router::new().route(
            "/api/user/self",
            axum::routing::get(|headers: axum::http::HeaderMap| async move {
                // 佔位符要真的換成憑證並送出
                let auth = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let uid = headers
                    .get("new-api-user")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                axum::Json(serde_json::json!({
                    "success": true,
                    "message": format!("auth={auth};uid={uid}"),
                    "data": {"group": "團隊方案", "quota": 500000, "used_quota": 100000, "total": 5}
                }))
            }),
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });

        let mut c = cfg(&format!("http://127.0.0.1:{port}/api/user/self"));
        c.headers = vec![
            HeaderPair {
                name: "Authorization".into(),
                value: "Bearer {{accessToken}}".into(),
            },
            HeaderPair {
                name: "New-Api-User".into(),
                value: "{{userId}}".into(),
            },
        ];
        c.access_token = "tok-123".into();
        c.user_id = "42".into();
        let client = reqwest::Client::new();
        let view = run(&client, &c, "sk-ignored", "https://unused").await;
        assert!(view.ok, "{}", view.message);
        assert_eq!(view.plan_name, "團隊方案");
        assert_eq!(view.remaining, Some(1.0));
        assert_eq!(view.used, Some(0.2));
        assert_eq!(view.percent, Some(20.0));
        assert!(
            view.extra.is_empty() || view.extra.contains("tok-123"),
            "extra 沒設時應為空"
        );
        // 上游看到的標頭要真的被展開（用 message 帶回來驗證）
        assert!(view.raw.contains("Bearer tok-123"), "{}", view.raw);
        assert!(view.raw.contains("uid=42"), "{}", view.raw);
    }

    /// 非 2xx 要回報狀態碼，而不是假裝成功。
    #[tokio::test]
    async fn non_2xx_is_reported() {
        let app = axum::Router::new().route(
            "/x",
            axum::routing::get(|| async {
                (axum::http::StatusCode::UNAUTHORIZED, "bad key")
            }),
        );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        let client = reqwest::Client::new();
        let view = run(&client, &cfg(&format!("http://127.0.0.1:{port}/x")), "k", "b").await;
        assert!(!view.ok);
        assert!(view.message.contains("401"), "{}", view.message);
    }

    /// 連不上／逾時要給人話，且不要拖過久。
    #[tokio::test]
    async fn connection_failure_is_readable() {
        let client = reqwest::Client::new();
        let mut c = cfg("http://127.0.0.1:9/nope");
        c.timeout_secs = 2;
        let view = run(&client, &c, "k", "b").await;
        assert!(!view.ok);
        // 實測：這台機器連 127.0.0.1:9 是**逾時**而不是立即拒絕（該埠可能被過濾），
        // 所以三種可讀說法都算過關：連不上／逾時／查詢失敗。重點是**不能是空訊息**。
        assert!(
            view.message.contains("連不上")
                || view.message.contains("逾時")
                || view.message.contains("查詢失敗"),
            "訊息要有可讀的原因：{}",
            view.message
        );
        assert!(!view.message.is_empty());
    }

    #[tokio::test]
    async fn empty_or_relative_url_is_rejected() {
        let client = reqwest::Client::new();
        let v = run(&client, &cfg("   "), "k", "b").await;
        assert!(!v.ok);
        assert!(v.message.contains("空的"), "{}", v.message);
        let v = run(&client, &cfg("/api/user/self"), "k", "b").await;
        assert!(!v.ok);
        assert!(v.message.contains("http"), "{}", v.message);
    }
}
