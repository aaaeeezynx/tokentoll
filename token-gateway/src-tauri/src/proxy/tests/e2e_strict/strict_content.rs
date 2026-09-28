//! 上游拒絕「非 assistant 角色空 content」的端到端測試。

    use super::super::*;

    #[tokio::test]
    async fn e2e_strict_upstream_rejects_empty_non_assistant_content() {
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
                            let invalid = body["messages"]
                                .as_array()
                                .is_some_and(|msgs| {
                                    msgs.iter().any(|m| {
                                        m.get("role").and_then(|r| r.as_str()) != Some("assistant")
                                            && m.get("content")
                                                .and_then(|c| c.as_str())
                                                .is_none_or(str::is_empty)
                                    })
                                });
                            *captured.lock().unwrap() = Some(body.0.clone());
                            if invalid {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "error": {
                                            "code": "400",
                                            "message": "message content is required for non-assistant roles",
                                            "param": "messages",
                                            "type": "invalid_request_error"
                                        }
                                    })),
                                )
                                    .into_response();
                            }
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
        let response = Client::new()
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "m",
                "input": [
                    {"type": "message", "role": "user", "content": "hi"},
                    {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                    {"type": "function_call_output", "call_id": "k1", "output": ""}
                ]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = captured.lock().unwrap().clone().unwrap();
        for message in body["messages"].as_array().unwrap() {
            if message["role"] != "assistant" {
                assert!(!message["content"].as_str().unwrap_or("").is_empty(), "{body}");
            }
        }
        gw.abort();
    }
