//! 端到端：跨格式轉譯（responses／anthropic）
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    /// Responses e2e：Codex 形狀 /v1/responses 進 → 翻譯成 chat 打上游 →
    /// 客戶端收到 Responses 形狀；流式含 created/delta/completed 事件；按實際模型記帳。
    #[tokio::test]
    async fn e2e_responses_to_chat() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("rs.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('nim','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'glm-test', 'actual-glm', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假上游：斷言收到 chat 形狀；回 chat 回包（流式回 SSE）
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "actual-glm");
                assert_eq!(body["messages"][0]["role"], "system");
                assert_eq!(body["messages"][0]["content"], "be nice");
                assert_eq!(body["messages"][1]["role"], "user");
                assert_eq!(body["messages"][1]["content"], "hi");
                assert_eq!(body["max_tokens"], 32);
                if body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false) {
                    let sse = "data: {\"id\":\"chatcmpl-rs1\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"he\"}}]}\n\ndata: {\"id\":\"chatcmpl-rs1\",\"choices\":[{\"delta\":{\"content\":\"llo\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"chatcmpl-rs1\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-rs1", "model": "actual-glm",
                    "choices": [{"message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 7, "completion_tokens": 3}
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
                "UPDATE providers SET base_url=?1 WHERE name='nim'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "rs".into(),
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

        // 1) 非流式：Responses 進 → Responses 出
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": "hi",
                "max_output_tokens": 32
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["object"], "response");
        assert_eq!(body["status"], "completed");
        assert_eq!(body["model"], "glm-test");
        assert_eq!(body["output"][0]["type"], "message");
        assert_eq!(body["output"][0]["content"][0]["text"], "hello");
        assert_eq!(body["usage"]["input_tokens"], 7);
        assert_eq!(body["usage"]["output_tokens"], 3);

        // 2) 流式：SSE 事件序列
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": "hi",
                "max_output_tokens": 32,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("event: response.created"));
        assert!(sse_body.contains("event: response.output_item.added"));
        assert!(sse_body.contains("response.output_text.delta"));
        assert!(sse_body.contains("\"delta\":\"he\""));
        assert!(sse_body.contains("event: response.output_item.done"));
        assert!(sse_body.contains("\"text\":\"hello\""));
        assert!(sse_body.contains("event: response.completed"));
        assert!(sse_body.contains("\"input_tokens\":7"));
        assert!(sse_body.contains("\"output_tokens\":3"));
        // 3) 記帳按實際送出模型
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let m: String = conn
            .query_row(
                "SELECT model_raw FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m, "actual-glm");
        gw.abort();
    }


    /// e2e：Codex 自訂（freeform）工具往返 + 上游錯誤改發 response.failed。
    #[tokio::test]
    async fn e2e_custom_tool_and_stream_error() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ct.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('nim','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'glm-test', 'actual-glm', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                // custom 工具必須 function 化送出
                let tools = body["tools"].as_array().unwrap();
                assert_eq!(tools.len(), 2, "exec_command + apply_patch");
                let ap = tools
                    .iter()
                    .find(|t| t["function"]["name"] == "apply_patch")
                    .expect("apply_patch 必須送出");
                assert_eq!(ap["type"], "function");
                assert_eq!(ap["function"]["parameters"]["required"][0], "input");
                // 歷史：custom_tool_call(_output) 已還原成 assistant tool_calls + tool
                let msgs = body["messages"].as_array().unwrap();
                let last_user = msgs
                    .iter()
                    .rev()
                    .find(|m| m["role"] == "user")
                    .unwrap();
                if last_user["content"] == "fail" {
                    let sse = "data: {\"error\":{\"message\":\"upstream boom\",\"type\":\"rate_limit_error\"}}\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                let has_custom_call = msgs.iter().any(|m| {
                    m.get("tool_calls")
                        .and_then(|c| c.as_array())
                        .is_some_and(|a| a.iter().any(|t| t["function"]["name"] == "apply_patch"))
                });
                assert!(has_custom_call, "custom 歷史必須還原成 tool_calls");
                assert!(msgs.iter().any(|m| m["role"] == "tool"), "tool 結果必須保留");
                let args = serde_json::json!({"input": "*** Begin Patch\n*** End Patch"}).to_string();
                let c1 = serde_json::json!({"id":"chatcmpl-ct1","choices":[{"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_9","function":{"name":"apply_patch","arguments":args}}]}}]}).to_string();
                let sse = format!(
                    "data: {c1}\n\ndata: {{\"id\":\"chatcmpl-ct1\",\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: {{\"id\":\"chatcmpl-ct1\",\"choices\":[],\"usage\":{{\"prompt_tokens\":5,\"completion_tokens\":4}}}}\n\ndata: [DONE]\n\n"
                );
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from(sse))
                    .unwrap()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='nim'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "ct".into(),
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

        let tools = serde_json::json!([
            {"type": "function", "name": "exec_command", "description": "run",
             "parameters": {"type": "object"}},
            {"type": "custom", "name": "apply_patch", "description": "patch files",
             "format": {"type": "grammar", "syntax": "lark", "definition": "start: x"}}
        ]);

        // 1) 流式：custom 工具呼叫 → custom_tool_call 還原
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": [
                    {"type": "message", "role": "user", "content": "hi"},
                    {"type": "custom_tool_call", "call_id": "ctc1", "name": "apply_patch",
                     "input": "*** Begin Patch\n*** End Patch"},
                    {"type": "custom_tool_call_output", "call_id": "ctc1",
                     "output": "Success. Updated the following files:\nM a.ts"}
                ],
                "tools": tools,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("\"type\":\"custom_tool_call\""), "{sse_body}");
        assert!(sse_body.contains("\"name\":\"apply_patch\""));
        assert!(sse_body.contains("*** Begin Patch"));
        assert!(sse_body.contains("event: response.completed"));
        assert!(!sse_body.contains("\"type\":\"function_call\""));

        // 2) 上游 error 事件 → response.failed（不得靜默 completed）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": [{"type": "message", "role": "user", "content": "fail"}],
                "tools": tools,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("event: response.failed"), "{sse_body}");
        assert!(sse_body.contains("upstream boom"));
        assert!(sse_body.contains("rate_limit_error"));
        assert!(!sse_body.contains("response.completed"));
        gw.abort();
    }


    /// 翻譯 e2e：Anthropic 進 → 假 OpenAI 出 → 客戶端收到 Anthropic 形狀 + 按實際模型記帳；
    /// 反向（OpenAI → Anthropic 渠道）明確 400。
    #[tokio::test]
    async fn e2e_anthropic_to_openai() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tr.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('tr','universal','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('an','universal','anthropic','http://127.0.0.1:1','k','anthropic','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='tr'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'claude-sonnet-4-6', 'deepseek-v4-flash', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假 OpenAI 上游：斷言收到轉換後的請求
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "deepseek-v4-flash");
                assert_eq!(body["messages"][0]["role"], "system");
                assert_eq!(body["messages"][1]["content"], "hi");
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-t1", "model": "deepseek-v4-flash",
                    "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 8, "completion_tokens": 2}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let (secret, an_secret) = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='tr'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let mk = |name: &str| {
                let pid: i64 = conn
                    .query_row("SELECT id FROM providers WHERE name=?1", [name], |r| {
                        r.get(0)
                    })
                    .unwrap();
                keys::create_key(
                    &conn,
                    &keys::KeyInput {
                        name: name.into(),
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
            (mk("tr"), mk("an"))
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) Anthropic 進 → Anthropic 出
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/messages"))
            .bearer_auth(&secret)
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-sonnet-4-6",
                "max_tokens": 16,
                "system": "sys",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["type"], "message");
        assert_eq!(body["model"], "claude-sonnet-4-6", "回顯請求模型");
        assert_eq!(body["content"][0]["text"], "ok");
        assert_eq!(body["usage"]["input_tokens"], 8);
        assert_eq!(body["usage"]["output_tokens"], 2);
        // 按實際模型記帳
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (m, itok, otok): (String, i64, i64) = conn
            .query_row(
                "SELECT model_raw, in_tok, out_tok FROM request_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((m.as_str(), itok, otok), ("deepseek-v4-flash", 8, 2));

        // 2) 反向（OpenAI 請求打 Anthropic 渠道）→ 明確 400
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&an_secret)
            .json(&serde_json::json!({"model": "x", "messages": []}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);

        gw.abort();
    }
