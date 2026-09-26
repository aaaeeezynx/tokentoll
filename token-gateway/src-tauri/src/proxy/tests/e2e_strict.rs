//! 端到端：嚴格上游、剝離重試、診斷可見性
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    /// 嚴格上游 400 unknown field → 剝離重發一次成功，且同渠道後續請求預先剝離。
    #[tokio::test]
    async fn e2e_strict_upstream_retry_strip() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let hits = Arc::new(AtomicUsize::new(0));
        let fake = {
            let hits = hits.clone();
            axum::Router::new().route(
                "/v1/responses",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let hits = hits.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            if body.get("verbosity").is_some() {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "error": {"message": "json: unknown field \"verbosity\" Request id: 1",
                                            "type": "BadRequest", "code": "InvalidParameter"}
                                    })),
                                )
                                    .into_response();
                            }
                            axum::Json(serde_json::json!({
                                "id": "resp-1", "model": "m",
                                "output": [{"type": "message", "content": [{"type": "output_text", "text": "ok"}]}],
                                "usage": {"input_tokens": 3, "output_tokens": 1}
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
                 VALUES ('s','universal','openai-responses',?1,'k','bearer','[]',0,1,0,0)",
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
        // Codex 式請求自帶 verbosity：首發 400 → 剝離重發 → 200
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": "hi", "verbosity": "low"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        // 第二次同樣請求只打一次（記憶生效）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": "hi", "verbosity": "low"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        gw.abort();
    }


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


    /// OpenRouter 口徑：Codex 自帶 prompt_cache_key，翻译後 chat 體首發被
    /// `Unsupported parameter(s)` 400 → 剝離重發 → 200（2026-09-24 codex 實例）。
    #[tokio::test]
    async fn e2e_openrouter_prompt_cache_key_strip() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let hits = Arc::new(AtomicUsize::new(0));
        let last_body: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let fake = {
            let hits = hits.clone();
            let last_body = last_body.clone();
            axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let hits = hits.clone();
                        let last_body = last_body.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            *last_body.lock().unwrap() = Some(body.0.clone());
                            if body.get("prompt_cache_key").is_some() {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "message": "Validation: Unsupported parameter(s): `prompt_cache_key`",
                                        "type": "Bad Request", "code": 400
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
        let http = Client::new();
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "m", "prompt_cache_key": "sess-1", "input": "hi"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        let fwd = last_body.lock().unwrap().clone().expect("上游應收到請求");
        assert!(fwd.get("prompt_cache_key").is_none(), "{fwd}");
        gw.abort();
    }


    /// 診斷中心的可見性閉環：**被網關拒絕的請求必須能在 `proxy_trace` 查到。**
    ///
    /// 使用者的原始症狀是「連線錯誤（通常是 400）」。這類 400 幾乎全部由
    /// `reject()` 產生（模型不在清單內、白名單、限流…）。如果 `reject()` 不寫
    /// trace，診斷中心就會**剛好漏掉最主要的症狀** —— 這個測試釘住那件事：
    /// 從「發一個會被拒的請求」到「診斷頁查得到它」的整條路。
    #[tokio::test]
    async fn e2e_rejected_request_is_visible_in_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('diag-src','codex','openai-chat','http://127.0.0.1:9','up-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='diag-src'", [], |r| r.get(0))
                .unwrap();
            crate::models::model_add(
                &conn,
                &crate::models::UsableModelInput {
                    provider_id: pid,
                    display_name: "known-model".into(),
                    actual_model: "known-model".into(),
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
                    name: "diag".into(),
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

        // ① 未登記模型 → 由 reject() 回 400（最常見的「連線錯誤」來源）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "ghost-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);

        // ② 缺少 Authorization → 401，同樣要留痕
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .json(&serde_json::json!({"model": "known-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);

        gw.abort();

        // 診斷查詢 —— 等同診斷中心三個面板呼叫的後端。
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let recent = trace::list_recent(&conn, 100).unwrap();
        let rejected: Vec<_> = recent.iter().filter(|r| r.trans_kind == "rejected").collect();
        let seen: Vec<(String, u16)> = recent
            .iter()
            .map(|r| (r.trans_kind.clone(), r.upstream_status))
            .collect();
        assert_eq!(rejected.len(), 2, "兩次拒絕都應留痕；實際 trace={seen:?}");

        // 問題清單只看 level=warn，故拒絕必須是 warn 才會出現在第一眼
        let problems = trace::list_problems(&conn, 100).unwrap();
        assert!(
            problems.iter().any(|r| r.upstream_status == 400),
            "400 應出現在問題清單；actual={:?}",
            problems.iter().map(|r| r.upstream_status).collect::<Vec<_>>()
        );
        assert!(
            problems.iter().any(|r| r.upstream_status == 401),
            "401 應出現在問題清單"
        );

        // note 要能讀懂「為什麼被拒」，且**不得含金鑰**
        let m400 = rejected
            .iter()
            .find(|r| r.upstream_status == 400)
            .expect("應有一筆 400");
        assert_eq!(m400.level, "warn", "拒絕應記為 warn");
        assert!(
            m400.note.contains("ghost-model"),
            "note 應說明被拒的模型：{}",
            m400.note
        );
        for row in &rejected {
            assert!(
                !row.note.contains(&secret),
                "trace 的 note 不得包含金鑰：{}",
                row.note
            );
        }
    }
