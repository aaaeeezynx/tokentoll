//! 端到端：認證、記錄、模型守門、改道
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    /// Google 方向 e2e：斷言上游收到 x-goog-api-key 且無 Bearer，usageMetadata 正確記帳。
    #[tokio::test]
    async fn e2e_goog_key_forwarding() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("g.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('g','universal','gemini','PLACEHOLDER','up-key','goog-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
        }
        let fake = axum::Router::new().route(
            "/v1beta/models/gemini-3.6-flash:generateContent",
            axum::routing::post(
                |headers: HeaderMap, body: axum::Json<serde_json::Value>| async move {
                    assert_eq!(
                        headers.get("x-goog-api-key").and_then(|v| v.to_str().ok()),
                        Some("up-key")
                    );
                    assert!(headers.get("authorization").is_none(), "不應透出 Bearer");
                    assert!(body.get("model").is_none());
                    axum::Json(serde_json::json!({
                        "candidates": [{"content": {"parts": [{"text": "ok"}]}}],
                        "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 1, "totalTokenCount": 6}
                    }))
                    .into_response()
                },
            ),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='g'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='g'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "g".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        let r = http
            .post(format!(
                "http://127.0.0.1:{gw_port}/v1beta/models/gemini-3.6-flash:generateContent?key=CLIENTKEY"
            ))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"contents": [{"parts": [{"text": "hi"}]}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (model, itok, otok): (String, i64, i64) = conn
            .query_row(
                "SELECT model_raw, in_tok, out_tok FROM request_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // 模型從 URL 回填，用量按 usageMetadata 解析
        assert_eq!((model.as_str(), itok, otok), ("gemini-3.6-flash", 5, 1));
        gw.abort();
    }


    /// 端到端：假上游 + 真網關 + 真 DB。覆蓋非流式轉發/鑑權/日誌三件套。
    #[tokio::test]
    async fn e2e_forward_auth_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // 註：此處原宣告 'openai-responses'，但本測試走的是 chat 端點 ——
            // 它之所以一直綠燈，正是因為 B6 的靜默直通掩蓋了設定錯誤。
            // 改為 'openai-chat' 以符合本測試「chat 直通轉發」的意圖。
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('fake','codex','openai-chat','PLACEHOLDER','up-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
        }

        // 假上游：stream=true 回 SSE，否則回固定 usage JSON
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                if body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false) {
                    let sse = "data: {\"model\":\"gpt-5\",\"choices\":[]}\n\ndata: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                axum::Json(serde_json::json!({
                    "model": "gpt-5",
                    "usage": {"prompt_tokens": 11, "completion_tokens": 6}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='fake'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            // 注意：基址無 /v1 路徑，入站 /v1/chat/completions 原樣保留
        }

        // 建 Key
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='fake'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "e2e".into(),
                    provider_id: Some(pid),
                    quota_tokens: 1000,
                    rate_limit_qpm: 60,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };

        // 起網關
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));

        let http = Client::new();
        // 1) 無鑑權 → 401
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
        // 2) 正常轉發
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["usage"]["prompt_tokens"], 11);
        // 2c) B6 端到端回歸：把渠道改成 responses 協議後，同一個 chat 請求必須
        //     得到網關自己產生的明確 400，而不是被靜默轉發給上游後失敗。
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET api_format='openai-responses' WHERE name='fake'",
                [],
            )
            .unwrap();
        }
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "chat→responses 應明確拒絕（B6）");
        let err_text = r.text().await.unwrap();
        assert!(
            err_text.contains("Responses"),
            "錯誤訊息應說明協議不符: {err_text}"
        );
        // 訊息必須是可理解的中文說明，而非上游原始錯誤
        assert!(
            err_text.contains("Chat 兼容渠道") || err_text.contains("Responses 客戶端"),
            "應給出可行動的建議: {err_text}"
        );
        // 還原協議，避免影響後續流式步驟
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET api_format='openai-chat' WHERE name='fake'",
                [],
            )
            .unwrap();
        }
        // 2b) 流式轉發：SSE 原樣透出 + 結束落日誌
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5", "stream": true}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("prompt_tokens"), "SSE 應透出上游分塊");
        // 流式日誌在後臺任務寫庫，輪詢等待
        let (app2, i2, o2, s2): (String, i64, i64, i64) = {
            let mut last = ("".to_string(), -1i64, -1i64, -1i64);
            for _ in 0..20 {
                let conn = crate::db::open_and_ensure(&db_path).unwrap();
                let row: Option<(String, i64, i64, i64, i64)> = conn
                    .query_row(
                        "SELECT app, in_tok, out_tok, status, is_stream FROM request_logs ORDER BY id DESC LIMIT 1",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                    )
                    .ok();
                if let Some((a, i, o, st, is)) = row {
                    if is == 1 {
                        last = (a, i, o, st);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            last
        };
        assert_eq!((app2.as_str(), i2, o2, s2), ("codex", 3, 2, 200));
        // 3) 非流式日誌落庫（取成功那一行；步驟 2c 的被拒 400 也會落一行，
        //    故必須以 status 區分 —— 這同時驗證「被拒請求也留痕」）
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let rejects: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM request_logs WHERE status=400",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rejects, 1, "B6 的 400 應留下一筆被拒日誌");
        let (app, itok, otok, status): (String, i64, i64, i64) = conn
            .query_row(
                "SELECT app, in_tok, out_tok, status FROM request_logs
                 WHERE is_stream=0 AND status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((app.as_str(), itok, otok, status), ("codex", 11, 6, 200));
        let used: i64 = conn
            .query_row("SELECT used_tokens FROM local_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(used, 17 + 5);
        // 4) 配額用盡 → 429
        conn.execute("UPDATE local_keys SET quota_tokens=22", []).unwrap();
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 429);

        gw.abort();
    }


    /// 跨來源防呆：Key 綁定來源登記了啟用模型時，未登記模型直接 400 指明來源，不轉發
    #[tokio::test]
    async fn e2e_unknown_model_blocked_with_hint() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('guard-src','codex','openai-chat','http://127.0.0.1:9','up-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='guard-src'", [], |r| r.get(0))
                .unwrap();
            crate::models::model_add(
                &conn,
                &crate::models::UsableModelInput {
                    provider_id: pid,
                    display_name: "real-model".into(),
                    actual_model: "real-model".into(),
                    context_window: None,
                    reasoning: "unset".into(),
                    ord: 0,
                    enabled: true,
                },
            )
            .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "e2e".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 60,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        // 未登記模型 → 400 且指明綁定來源
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "ghost-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        let body = r.text().await.unwrap();
        assert!(body.contains("guard-src"), "應指明綁定來源：{body}");
        assert!(body.contains("ghost-model"), "應指明請求模型：{body}");
        // 已登記模型 → 通過防呆（上游 127.0.0.1:9 不可達，轉發失敗非 400 即可）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "real-model"}))
            .send()
            .await
            .unwrap();
        assert_ne!(r.status(), 400);
        gw.abort();
    }


    /// 跨來源路由 e2e：模型登記在來源 B、Key 綁來源 A → 請求改走 B（帶 B 的金鑰）；
    /// 兩邊都沒有的模型 → 400 防呆（A 已登記模型時）。
    #[tokio::test]
    async fn e2e_model_reroute_cross_provider() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("rr.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // A：Key 綁定來源；基址故意不可達（若未改走 B 會 502，作金絲雀）
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pa','universal','openai-chat','http://127.0.0.1:9','up-key-a','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pa: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pa'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'a-model', 'a-model', 'unset', 0, 1)",
                [pa],
            )
            .unwrap();
        }
        // 假上游 B：回顯收到的模型與上游鑑權（直通按入站路徑原樣轉發）
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(
                |headers: axum::http::HeaderMap,
                 body: axum::Json<serde_json::Value>| async move {
                    let auth = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    axum::Json(serde_json::json!({
                        "model": body.get("model").cloned().unwrap_or_default(),
                        "up_auth": auth,
                        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
                    }))
                    .into_response()
                },
            ),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pb','universal','openai-chat',?1,'up-key-b','bearer','[]',1,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pb: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pb'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'or-model', 'or-model', 'unset', 0, 1)",
                [pb],
            )
            .unwrap();
            let pa: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pa'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "rr".into(),
                    provider_id: Some(pa),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) or-model 只登記在 B → 改走 B，帶 B 的金鑰
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "or-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["model"], "or-model");
        assert_eq!(body["up_auth"], "Bearer up-key-b", "應改走來源 B");
        // 2) 兩邊都沒有的模型 → 400 防呆（A 已登記啟用模型）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "nowhere-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        // 3) 記帳落在路由命中的來源 B
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (pid,): (i64,) = conn
            .query_row(
                "SELECT provider_id FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();
        let pb: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='pb'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pid, pb);
        gw.abort();
    }


    /// 直通 e2e：OpenAI 進 → display 名改寫為 actual 再轉發；
    /// 未映射模型命中跨來源防呆 400；記帳用實際送出的模型。
    #[tokio::test]
    async fn e2e_passthrough_resolves_display_to_actual() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pt.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pt','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pt'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'deepseek-v4-flash-0731', 'deepseek-ai/deepseek-v4-flash-0731', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假上游：回顯收到的模型名
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                axum::Json(serde_json::json!({
                    "model": body.get("model").cloned().unwrap_or_default(),
                    "usage": {"prompt_tokens": 5, "completion_tokens": 1}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='pt'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pt'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "pt".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) 顯示名進 → 上游收到 actual
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "deepseek-v4-flash-0731"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["model"], "deepseek-ai/deepseek-v4-flash-0731");
        // 2) 未映射模型 → 跨來源防呆 400，不轉發
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "gpt-9"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        // 3) 成功請求按實際模型記帳
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let m: String = conn
            .query_row(
                "SELECT model_raw FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m, "deepseek-ai/deepseek-v4-flash-0731");
        gw.abort();
    }


    /// 並行 function_call 經 responses→chat 翻譯後必須是一條 assistant 帶全部
    /// tool_calls（逐條拆分會被嚴格上游以「其後非 tool 消息」拒收，見 Zen 400）。
    #[tokio::test]
    async fn e2e_parallel_calls_forwarded_merged() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let captured: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let fake = {
            let captured = captured.clone();
            axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let captured = captured.clone();
                        async move {
                            *captured.lock().unwrap() = Some(body.0);
                            axum::Json(serde_json::json!({
                                "id": "chatcmpl-1", "object": "chat.completion",
                                "created": 1, "model": "m",
                                "choices": [{"index": 0,
                                    "message": {"role": "assistant", "content": "ok"},
                                    "finish_reason": "stop"}],
                                "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                            }))
                            .into_response()
                        }
                    },
                ),
            )
        };
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('s','universal','openai-chat',?1,'k','bearer','[]',0,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='s'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "s".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        // Codex 舊 session 重放：連發 3 個 function_call + 3 個 output
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "function_call", "call_id": "k0", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k2", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "k0", "output": "r0"},
                {"type": "function_call_output", "call_id": "k1", "output": "r1"},
                {"type": "function_call_output", "call_id": "k2", "output": "r2"}
            ]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let fwd = captured.lock().unwrap().clone().expect("上游應收到請求");
        let msgs = fwd["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 5, "應為 user+assistant+tool*3，實際：{msgs:?}");
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[1]["tool_calls"].as_array().unwrap().len(), 3);
        assert_eq!(msgs[1]["tool_calls"][0]["id"], "k0");
        assert_eq!(msgs[1]["tool_calls"][2]["id"], "k2");
        assert_eq!(msgs[2]["tool_call_id"], "k0");
        assert_eq!(msgs[4]["tool_call_id"], "k2");
        gw.abort();
    }
