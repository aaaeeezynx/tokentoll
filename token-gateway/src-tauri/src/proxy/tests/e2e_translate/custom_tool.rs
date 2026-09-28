//! Codex 自訂（freeform）工具與上游串流錯誤的端到端測試。

    use super::super::*;

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
