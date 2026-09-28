//! 未登記模型守門與跨來源改道的端到端測試。

    use super::super::*;

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
