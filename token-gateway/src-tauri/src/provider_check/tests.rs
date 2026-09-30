//! `provider_check` 的測試：位址正規化、狀態碼分類、以及**真的對本機伺服器打一次**。

use super::*;

#[test]
fn normalize_url_adds_scheme_and_strips_slash() {
    assert_eq!(
        normalize_url("https://api.deepseek.com/v1/").as_deref(),
        Some("https://api.deepseek.com/v1")
    );
    assert_eq!(
        normalize_url("  api.openai.com/v1  ").as_deref(),
        Some("https://api.openai.com/v1"),
        "沒寫 scheme 要補 https"
    );
    assert_eq!(
        normalize_url("localhost:11434/v1").as_deref(),
        Some("http://localhost:11434/v1"),
        "本機服務補 http（不是 https）"
    );
    assert_eq!(
        normalize_url("127.0.0.1:1234/v1").as_deref(),
        Some("http://127.0.0.1:1234/v1")
    );
}

#[test]
fn normalize_url_rejects_junk() {
    for bad in ["", "   ", "https://", "ftp://example.com", "沒有這個東西"] {
        assert!(normalize_url(bad).is_none(), "{bad:?} 應被拒絕");
    }
}

#[test]
fn classify_treats_auth_errors_as_reachable() {
    // 這條是這個功能的定位：401／403／404 都代表「主機活著」
    for ok in [200u16, 204, 301, 400, 401, 403, 404, 405, 429, 500, 503] {
        let (msg, reachable) = classify(ok);
        assert!(reachable, "{ok} 應算可達（{msg}）");
        assert!(!msg.is_empty());
    }
    let (msg, reachable) = classify(0);
    assert!(!reachable);
    assert!(msg.contains("連不上"), "{msg}");
    // 500 系列要講清楚是上游的問題，而不是「一切正常」
    assert!(classify(503).0.contains("伺服器端問題"));
    // 401 要點出「需要金鑰」，否則使用者會以為壞了
    assert!(classify(401).0.contains("需要金鑰"));
}

/// 真的起一個本機伺服器，驗證「可達」這條路（不是只有純函式被測到）。
#[tokio::test]
async fn check_reports_reachable_for_live_server() {
    let app = axum::Router::new().route(
        "/v1/models",
        axum::routing::get(|| async { axum::Json(serde_json::json!({"ok": true})) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/v1/models");
    let r = check_with(&client, &url).await;
    assert!(r.reachable, "{r:?}");
    assert_eq!(r.status, 200);
    assert_eq!(r.url, url);
    assert!(r.message.contains("可達"), "{}", r.message);
}

/// 連不上的埠（9 是 discard）要回報不可達，而且要有人話。
#[tokio::test]
async fn check_reports_unreachable_for_dead_port() {
    let client = reqwest::Client::new();
    let r = check_with(&client, "http://127.0.0.1:9/v1/models").await;
    assert!(!r.reachable, "{r:?}");
    assert_eq!(r.status, 0);
    assert!(r.message.contains("連不上"), "{}", r.message);
}

/// 命令本身：壞位址要給得出「請先填寫」而不是 panic。
#[tokio::test]
async fn command_rejects_empty_url() {
    let e = provider_check("   ".to_string()).await.unwrap_err();
    assert!(e.contains("請先填寫"), "{e}");
}
