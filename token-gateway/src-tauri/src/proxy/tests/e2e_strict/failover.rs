//! 端到端：協議自動回退與來源換手。
//!
//! 兩個真實世界的症狀：
//! ① opencode-go 的 `grok-4.7`／`muse-spark` 只在 `/responses` 上架，但來源宣告
//!    `openai-chat` → Codex 的請求被翻成 chat 後上游回 `ModelProtocolUnsupported`。
//! ② 來源連不上（打錯埠、服務沒開）時，佇列裡還有別的來源登記了同一個模型。

    use super::super::*;

    fn insert_provider(
        conn: &rusqlite::Connection,
        name: &str,
        api_format: &str,
        base_url: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
             VALUES (?1,'codex',?2,?3,'up-key','bearer','[]',0,1,0,0)",
            rusqlite::params![name, api_format, base_url],
        )
        .unwrap();
        conn.query_row("SELECT id FROM providers WHERE name=?1", [name], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn add_model(conn: &rusqlite::Connection, pid: i64, actual: &str) {
        conn.execute(
            "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
             VALUES (?1, ?2, ?2, 'unset', 0, 1)",
            rusqlite::params![pid, actual],
        )
        .unwrap();
    }

    fn make_key(conn: &rusqlite::Connection, pid: i64, name: &str) -> String {
        keys::create_key(
            conn,
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
    }

    /// ① Codex（Responses 入站）遇到「來源宣告 chat、模型只在 responses 上架」時，
    /// 要自動換協議重試一次並成功。
    #[tokio::test]
    async fn e2e_responses_inbound_falls_back_to_responses_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fo1.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // 名稱刻意不叫 `zen`：`open_and_ensure` 會播種一個叫 zen（api_format=mixed）
            // 的來源，撞名會讓下面的 `SELECT id WHERE name=…` 拿到種子列而不是這一列。
            let pid = insert_provider(&conn, "fo-chat-src", "openai-chat", "PLACEHOLDER");
            add_model(&conn, pid, "grok-test");
            make_key(&conn, pid, "fo1")
        };
        // 假上游：chat 端點回真實原文的「不支援本協議」；responses 端點正常回應。
        // 用 fallback 接住所有路徑：翻譯路徑是 `{base}/chat/completions`，
        // 直通路徑是 `{base}/v1/responses`，兩者前綴不同，寫死 route 容易漏。
        // 記下假上游實際收到的路徑順序：這是「先打 chat 被拒、才換 responses」的證據。
        let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_fake = seen.clone();
        let fake = axum::Router::new().fallback(
            move |uri: axum::http::Uri, body: axum::Json<serde_json::Value>| {
                let seen = seen_fake.clone();
                async move {
                    seen.lock().unwrap().push(uri.path().to_string());
                    if uri.path().ends_with("/responses") {
                        assert_eq!(body["model"], "grok-test");
                        assert_eq!(body["input"], "hi");
                        return axum::Json(serde_json::json!({
                            "id": "resp_1",
                            "object": "response",
                            "status": "completed",
                            "model": "grok-test",
                            "output": [{
                                "type": "message",
                                "role": "assistant",
                                "content": [{"type": "output_text", "text": "hello-from-responses"}]
                            }],
                            "usage": {"input_tokens": 5, "output_tokens": 2, "total_tokens": 7}
                        }))
                        .into_response();
                    }
                    (
                        axum::http::StatusCode::BAD_REQUEST,
                        axum::Json(serde_json::json!({
                            "type": "error",
                            "error": {
                                "type": "ModelProtocolUnsupported",
                                "message": "Model does not support this protocol."
                            }
                        })),
                    )
                        .into_response()
                }
            },
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='fo-chat-src'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
        }
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "grok-test", "input": "hi", "stream": false}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "協議回退後應該成功");
        let text = r.text().await.unwrap();
        assert!(text.contains("hello-from-responses"), "{text}");

        // 證據：假上游實際收到的路徑順序。第一個請求必須「先 chat（被拒）才 responses」。
        let seq = seen.lock().unwrap().clone();
        assert!(
            seq.iter().any(|p| p.ends_with("/chat/completions")),
            "第一個候選應該是 chat：{seq:?}"
        );
        assert_eq!(
            seq.iter().filter(|p| p.ends_with("/responses")).count(),
            1,
            "responses 只該被呼叫一次：{seq:?}"
        );
        // 第二次請求：學到的協議要讓它第一個就打 responses（chat 不再被試）。
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "grok-test", "input": "hi", "stream": false}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let seq2 = seen.lock().unwrap().clone();
        assert_eq!(
            seq2.iter()
                .filter(|p| p.ends_with("/chat/completions"))
                .count(),
            1,
            "學到協議之後不該再試 chat：{seq2:?}"
        );
        assert_eq!(
            seq2.iter().filter(|p| p.ends_with("/responses")).count(),
            2,
            "{seq2:?}"
        );
        gw.abort();
    }

    /// ② 來源連不上時，換到下一個登記了同一個模型的來源（failover queue）。
    #[tokio::test]
    async fn e2e_failover_to_next_provider_when_first_is_dead() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fo2.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // 9 是 discard 埠：連線必定被拒（既有測試也用這個手法）。
            let dead = insert_provider(&conn, "dead-src", "openai-chat", "http://127.0.0.1:9");
            add_model(&conn, dead, "same-model");
            make_key(&conn, dead, "fo2")
        };
        let fake = axum::Router::new().fallback(
            |uri: axum::http::Uri, body: axum::Json<serde_json::Value>| async move {
                assert!(
                    uri.path().contains("chat/completions"),
                    "備援來源收到非 chat 路徑：{}",
                    uri.path()
                );
                assert_eq!(body["model"], "same-model");
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-fo2",
                    "model": "same-model",
                    "choices": [{"message": {"role": "assistant", "content": "from-backup-src"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 4, "completion_tokens": 2}
                }))
                .into_response()
            },
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            let backup = insert_provider(
                &conn,
                "backup-src",
                "openai-chat",
                &format!("http://127.0.0.1:{fake_port}"),
            );
            add_model(&conn, backup, "same-model");
        }
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "same-model", "messages": [{"role": "user", "content": "hi"}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "第一個來源連不上時應該換手");
        let text = r.text().await.unwrap();
        assert!(text.contains("from-backup-src"), "{text}");
        gw.abort();
    }
