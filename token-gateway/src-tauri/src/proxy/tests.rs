    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn stream_options_only_for_chat() {
        assert!(should_inject_usage(false, "/v1/chat/completions"));
        assert!(should_inject_usage(true, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1/responses"));
        assert!(!should_inject_usage(false, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1beta/models/x:generateContent"));
    }

    #[test]
    fn unknown_field_parse_and_strip() {
        assert_eq!(
            parse_unknown_fields(r#"json: unknown field "verbosity" Request id: 1"#),
            vec!["verbosity"]
        );
        // JSON 轉義形態（真實回包樣子）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"json: unknown field \"verbosity\" Request id: 1"}"#),
            vec!["verbosity"]
        );
        assert!(parse_unknown_fields("plain 400").is_empty());
        assert!(parse_unknown_fields("unknown field \"\"").is_empty());
        let body = br#"{"model":"m","verbosity":"low","stream":true}"#;
        let stripped = strip_json_field(body, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("verbosity").is_none());
        assert_eq!(v["model"], "m");
        assert!(strip_json_field(body, "nope").is_none());
        assert!(strip_json_field(b"not json", "verbosity").is_none());
        // Codex 式：verbosity 藏在 text 裡，清空後 text 一併移除
        let nested = br#"{"model":"m","text":{"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("text").is_none());
        let nested2 = br#"{"model":"m","text":{"format":{"type":"text"},"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested2, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v["text"]["format"]["type"] == "text");
    }

    #[test]
    fn openrouter_unsupported_parameter_parse() {
        // OpenRouter 真實口徑（2026-09-24 codex 實例：prompt_cache_key 被拒）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"Validation: Unsupported parameter(s): 'prompt_cache_key'","type":"Bad Request","code":400}"#),
            vec!["prompt_cache_key"]
        );
        // 多個一次全取出
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): 'a', 'b-c_d'"),
            vec!["a", "b-c_d"]
        );
        assert_eq!(
            parse_unknown_fields("Validation: Unsupported parameter(s): `prompt_cache_key`"),
            vec!["prompt_cache_key"]
        );
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): \"a\", 'b'"),
            vec!["a", "b"]
        );
        assert!(parse_unknown_fields("Unsupported parameter(s): none").is_empty());
        // 剝離後重發體：prompt_cache_key 移除，其餘不動
        let body = br#"{"model":"m","prompt_cache_key":"sess-1","messages":[]}"#;
        let stripped = strip_json_field(body, "prompt_cache_key").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("prompt_cache_key").is_none());
        assert_eq!(v["model"], "m");
    }

    #[test]
    fn join_upstream_dedups_v1() {
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/chat/completions"),
            "https://router.flatkey.ai/v1/chat/completions"
        );
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/messages"),
            "https://router.flatkey.ai/v1/messages"
        );
        assert_eq!(
            join_upstream("https://host", "/v1/models"),
            "https://host/v1/models"
        );
    }

    #[test]
    fn normalize_model_rules() {
        assert_eq!(normalize_model("anthropic/claude-opus-4-8"), "claude-opus-4-8");
        assert_eq!(normalize_model("GPT-5.2-codex@low"), "gpt-5.2-codex-low");
        // -v1 / 日期 / 渠道尾綴剝除（短 ID 兜底定價）
        assert_eq!(
            normalize_model("global.anthropic.claude-opus-4-8-v1:0"),
            "global.anthropic.claude-opus-4-8"
        );
        assert_eq!(
            normalize_model("deepseek-v4-flash-ga-260731"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            normalize_model("gpt-4o-mini-2024-07-18"),
            "gpt-4o-mini"
        );
        assert_eq!(
            normalize_model("moonshotai/kimi-k2-0905:exa"),
            "kimi-k2-0905"
        );
        // 語義編號不受影響
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_model("llama-3"), "llama-3");
        assert_eq!(normalize_model("kimi-k2-turbo"), "kimi-k2-turbo");
        assert_eq!(normalize_model("kimi-k2-0905"), "kimi-k2-0905");
        assert_eq!(normalize_model("  kimi-k2-0905[1m] "), "kimi-k2-0905");
    }

    #[test]
    fn extract_openai_chat_usage() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"model":"gpt-5","usage":{"prompt_tokens":10,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":3}}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 5, 3));
        assert_eq!(u.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn extract_anthropic_stream_usage() {
        let start: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_start","message":{"model":"claude-opus-4-8","usage":{"input_tokens":7,"cache_read_input_tokens":2}}}"#,
        )
        .unwrap();
        let delta: serde_json::Value = serde_json::from_str(
            r#"{"type":"content_block_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        // message_delta 形狀
        let delta2: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&start, &mut u);
        extract_usage(&delta, &mut u);
        extract_usage(&delta2, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (7, 4, 2));
    }

    #[test]
    fn sse_accumulates_across_chunks() {
        let mut acc = SseAcc::new();
        acc.feed(b"data: {\"model\":\"gpt-5\",\"choices\":[]}\n\ndata: ");
        acc.feed(b"{\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n");
        assert_eq!((acc.usage.in_tok, acc.usage.out_tok), (10, 5));
        assert_eq!(acc.usage.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn infer_app_rules() {
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static("codex-cli/1.0"));
        assert_eq!(infer_app(&h), "codex");
        h.insert("x-tg-app", HeaderValue::from_static("hermes"));
        assert_eq!(infer_app(&h), "hermes");
        h.insert("x-tg-app", HeaderValue::from_static("nope"));
        assert_eq!(infer_app(&h), "codex", "非法聲明回退到 UA");
        h.insert("x-tg-app", HeaderValue::from_static("cursor"));
        assert_eq!(infer_app(&h), "cursor");
        let mut h2 = HeaderMap::new();
        h2.insert(
            "user-agent",
            HeaderValue::from_static("antigravity/1.0"),
        );
        assert_eq!(infer_app(&h2), "antigravity");
    }

    #[test]
    fn extract_gemini_usage_metadata() {
        // 完整形狀：candidates + thoughts 併入 out
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":1,"totalTokenCount":64,"thoughtsTokenCount":58}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 59));
        // 被截斷回包：無 candidates，用 total-prompt 回退
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"totalTokenCount":18,"thoughtsTokenCount":13}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 13));
        // 快取欄位
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":2,"totalTokenCount":12,"cachedContentTokenCount":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 2, 4));
    }

    #[test]
    fn strip_key_and_model_from_path() {
        assert_eq!(
            strip_key_param(Some("alt=sse&key=SECRET&x=1")).as_deref(),
            Some("alt=sse&x=1")
        );
        assert_eq!(strip_key_param(Some("key=SECRET")), None);
        assert_eq!(strip_key_param(None), None);
        assert_eq!(
            model_from_path("/v1beta/models/gemini-3.6-flash:generateContent").as_deref(),
            Some("gemini-3.6-flash")
        );
        assert_eq!(model_from_path("/v1/chat/completions"), None);
    }

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

    #[test]
    fn strip_unsupported_tools_keeps_function() {
        let body = serde_json::json!({
            "model": "deepseek-v4-flash",
            "tools": [
                {"type": "function", "function": {"name": "web_search", "parameters": {}}},
                {"type": "custom", "name": "computer_use", "parameters": {}},
                {"type": "custom", "name": "shell", "parameters": {}}
            ],
            "tool_choice": {"type": "function", "function": {"name": "web_search"}},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let tools = v["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        // tool_choice 仍存在（引用的 tool 還在）
        assert!(v.get("tool_choice").is_some());
    }

    #[test]
    fn strip_unsupported_tools_removes_all() {
        let body = serde_json::json!({
            "model": "m",
            "tools": [
                {"type": "custom", "name": "a"},
                {"type": "shell", "name": "b"}
            ],
            "tool_choice": "required",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("tools").is_none());
        assert!(v.get("tool_choice").is_none());
    }

    #[test]
    fn strip_unsupported_tools_no_tools() {
        let body = serde_json::json!({"model": "m", "messages": []});
        let bytes = serde_json::to_vec(&body).unwrap();
        // 沒有 tools 欄位，原樣返回
        assert!(strip_unsupported_tools(&bytes).is_none());
    }

    #[test]
    fn passthrough_chat_sanitize_repairs_and_noops() {        // 孤兒 tool_calls（舊中斷 session）→ 補佔位
        let broken = br#"{"model":"m","messages":[
            {"role":"user","content":"hi"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"f","arguments":"{}"}}]}
        ]}"#;
        let out = sanitize_passthrough_chat_body(broken, InFmt::OpenAi, false).expect("應修復");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "c1");
        // 有效歷史一字不動
        let ok_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
        assert!(sanitize_passthrough_chat_body(ok_body, InFmt::OpenAi, false).is_none());
        // 非 chat / 已翻譯不碰
        assert!(sanitize_passthrough_chat_body(broken, InFmt::Anthropic, false).is_none());
        assert!(sanitize_passthrough_chat_body(broken, InFmt::OpenAi, true).is_none());
        assert!(sanitize_passthrough_chat_body(b"not json", InFmt::OpenAi, false).is_none());
    }

    // ───────── B1：encrypted_content 剝離（Responses 直通路徑）─────────

    #[test]
    fn strip_encrypted_content_removes_nested_occurrences() {
        // 第三方 responses 端點會以「encrypted_content was not issued to this
        // caller」400 拒收，故直通前必須剝離所有副本（含巢狀）。
        let body = br#"{"model":"m","input":[
            {"type":"reasoning","encrypted_content":"SECRET","summary":[]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}],
             "meta":{"encrypted_content":"ALSO_SECRET"}}
        ],"store":false}"#;
        let out = strip_encrypted_content(body).expect("應命中並剝離");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "仍有殘留: {s}");
        assert!(!s.contains("SECRET"), "簽章值未移除: {s}");
        // 其餘欄位一字不動
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["model"], "m");
        assert_eq!(v["store"], false);
        assert_eq!(v["input"].as_array().unwrap().len(), 2);
        assert_eq!(v["input"][0]["type"], "reasoning");
        assert_eq!(v["input"][1]["content"][0]["text"], "hi");
    }

    #[test]
    fn strip_encrypted_content_is_noop_when_absent() {
        // 未命中必須回 None（呼叫端據此判斷「原文一字不動」）
        assert!(strip_encrypted_content(br#"{"model":"m","input":"hi"}"#).is_none());
    }

    #[test]
    fn strip_encrypted_content_handles_arrays_and_deep_nesting() {
        let body =
            br#"{"a":[[{"encrypted_content":"x"}]],"b":{"c":{"d":{"encrypted_content":"y"}}}}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "深層殘留: {s}");
    }

    #[test]
    fn strip_encrypted_content_leaves_similar_keys_alone() {
        // 只剝離精確鍵名，不得誤傷其他含相似字串的鍵
        let body = br#"{"my_encrypted_content_x":1,"encrypted_content":"z"}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("encrypted_content").is_none());
        assert_eq!(v["my_encrypted_content_x"], 1, "相似鍵名被誤刪");
    }

    #[test]
    fn strip_encrypted_content_ignores_non_json() {
        assert!(strip_encrypted_content(b"not json").is_none());
        assert!(strip_encrypted_content(b"").is_none());
    }

    // ───────── B5：格式矩陣窮舉（30 格全釘死）─────────

    #[test]
    fn trans_kind_matrix_is_fully_pinned() {
        use InFmt::*;
        use TargetFmt as T;
        const NONE: Option<TransKind> = Some(TransKind::None);
        const A2C: Option<TransKind> = Some(TransKind::AnthropicToChat);
        const R2C: Option<TransKind> = Some(TransKind::ResponsesToChat);
        // 5 種入站 × 6 種渠道協議 = 30 格，全部明列。
        // 這份表就是行為契約：任何分支被重排或漏改，這裡立刻失敗。
        let table: &[(InFmt, T, Option<TransKind>)] = &[
            // Anthropic 入站
            (Anthropic, T::OpenAiChat, A2C),
            (Anthropic, T::OpenAiResponses, None),
            (Anthropic, T::Mixed, A2C),
            (Anthropic, T::Anthropic, NONE),
            (Anthropic, T::Gemini, None),
            (Anthropic, T::Unknown, None),
            // Responses 入站（Codex）
            (Responses, T::OpenAiChat, R2C),
            (Responses, T::OpenAiResponses, NONE),
            (Responses, T::Mixed, R2C),
            (Responses, T::Anthropic, None),
            (Responses, T::Gemini, None),
            (Responses, T::Unknown, NONE),
            // OpenAI Chat 入站
            (OpenAi, T::OpenAiChat, NONE),
            (OpenAi, T::OpenAiResponses, None), // B6：原為靜默直通
            (OpenAi, T::Mixed, NONE),
            (OpenAi, T::Anthropic, None),
            (OpenAi, T::Gemini, None),
            (OpenAi, T::Unknown, NONE),
            // Gemini 原生入站
            (Gemini, T::OpenAiChat, None),
            (Gemini, T::OpenAiResponses, None),
            (Gemini, T::Mixed, None),
            (Gemini, T::Anthropic, None),
            (Gemini, T::Gemini, NONE),
            (Gemini, T::Unknown, None),
            // 未知入站（中性路徑，如 /v1/models）
            (Unknown, T::OpenAiChat, NONE),
            (Unknown, T::OpenAiResponses, NONE),
            (Unknown, T::Mixed, NONE),
            (Unknown, T::Anthropic, NONE),
            (Unknown, T::Gemini, None),
            (Unknown, T::Unknown, NONE),
        ];
        assert_eq!(table.len(), 30, "矩陣必須窮舉 5×6 共 30 格");
        for (i, t, want) in table {
            let got = resolve_trans_kind(*i, *t);
            match want {
                Some(k) => assert_eq!(got.ok(), Some(*k), "({i:?}, {t:?}) 翻譯類型不符"),
                None => assert!(got.is_err(), "({i:?}, {t:?}) 應被明確拒絕"),
            }
        }
    }

    #[test]
    fn b6_chat_to_responses_endpoint_errors_loudly() {
        // 原實作落入 `_ => TransKind::None`，把 chat 請求原樣打到 responses 端點，
        // 必然失敗且訊息來自上游、難以理解。改為網關自己明確報錯。
        let e = resolve_trans_kind(InFmt::OpenAi, TargetFmt::OpenAiResponses)
            .expect_err("應明確拒絕而非靜默直通");
        assert_eq!(e, E_CHAT_TO_RESPONSES);
        assert!(e.contains("Responses"), "訊息應指出協議不符: {e}");
    }

    #[test]
    fn matrix_preserves_original_error_messages() {
        // 逐格核對既有 400 訊息語義不變（含原實作的分支優先序）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::OpenAiResponses).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        // anthropic 入站的訊息優先於「gemini 渠道」訊息（原實作順序語義）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::Gemini).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Responses, TargetFmt::Anthropic).unwrap_err(),
            E_REVERSE_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Gemini, TargetFmt::OpenAiChat).unwrap_err(),
            E_GEMINI_IN_ONLY
        );
        assert_eq!(
            resolve_trans_kind(InFmt::OpenAi, TargetFmt::Gemini).unwrap_err(),
            E_GEMINI_OUT_ONLY
        );
    }

    #[test]
    fn in_fmt_from_path_matches_legacy_detection() {
        // 與原 detect_in_format 的判定完全一致
        assert_eq!(InFmt::from_path("/v1/messages"), InFmt::Anthropic);
        assert_eq!(InFmt::from_path("/v1/responses"), InFmt::Responses);
        assert_eq!(InFmt::from_path("/v1/chat/completions"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/models"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/embeddings"), InFmt::OpenAi);
        assert_eq!(
            InFmt::from_path("/v1beta/models/gemini-2.5-pro:generateContent"),
            InFmt::Gemini
        );
        assert_eq!(
            InFmt::from_path("/v1beta/models/x:streamGenerateContent"),
            InFmt::Gemini
        );
        // 中性路徑 → Unknown（透傳，不誤傷）
        assert_eq!(InFmt::from_path("/v1/health"), InFmt::Unknown);
    }

    #[test]
    fn target_fmt_from_db_covers_every_ui_value() {
        // 前端 Providers.tsx 的 API_FORMATS 必須全部被辨識，
        // 否則會落入 Unknown 而被拒服務。
        for f in [
            "openai-chat",
            "openai-responses",
            "mixed",
            "anthropic",
            "gemini",
        ] {
            assert_ne!(TargetFmt::from_db(f), TargetFmt::Unknown, "{f} 未被辨識");
            // as_str 應可往返
            assert_eq!(TargetFmt::from_db(f).as_str(), f);
        }
        assert_eq!(TargetFmt::from_db("typo"), TargetFmt::Unknown);
        assert_eq!(TargetFmt::from_db(""), TargetFmt::Unknown);
    }

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
