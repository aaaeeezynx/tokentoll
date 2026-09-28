//! display→actual 改名與並行 tool_calls 合併的端到端測試。

    use super::super::*;

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
