//! Google 金鑰轉發，以及非流式轉發／鑑權／日誌三件套的端到端測試。

    use super::super::*;
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
