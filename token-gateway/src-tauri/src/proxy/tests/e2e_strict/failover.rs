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

    /// ③ 斷路器：同一個死來源連續失敗 3 次後就不再優先嘗試它。
    ///
    /// 觀測點是**這個測試自己的資料庫**：連接失敗會寫一筆 `upstream_status=502`
    /// 的追蹤（`log_reject`），所以「502 的筆數有沒有再增加」就是「有沒有再去撞
    /// 那個死來源」的直接證據。
    #[tokio::test]
    async fn e2e_circuit_breaker_stops_probing_dead_source() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fo3.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            let dead = insert_provider(&conn, "brk-dead", "openai-chat", "http://127.0.0.1:9");
            add_model(&conn, dead, "brk-model");
            make_key(&conn, dead, "fo3")
        };
        let fake = axum::Router::new().fallback(
            |_uri: axum::http::Uri, body: axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "brk-model");
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-fo3",
                    "model": "brk-model",
                    "choices": [{"message": {"role": "assistant", "content": "from-backup"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 3, "completion_tokens": 1}
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
                "brk-backup",
                "openai-chat",
                &format!("http://127.0.0.1:{fake_port}"),
            );
            add_model(&conn, backup, "brk-model");
        }
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        let call = || {
            http.post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
                .bearer_auth(&secret)
                .json(&serde_json::json!({"model": "brk-model", "messages": [{"role": "user", "content": "hi"}]}))
        };

        // 502 追蹤筆數（= 撞了幾次死來源）
        let count_502 = || -> i64 {
            let conn = open_conn(&db_path).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM proxy_trace WHERE upstream_status=502",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };

        // 前三次：死來源都排在前面被撞一次 → 502 逐次增加，但使用者一律拿到 200。
        for i in 1..=3 {
            let r = call().send().await.unwrap();
            assert_eq!(r.status(), 200, "第 {i} 次應該換手成功");
            let t = r.text().await.unwrap();
            assert!(t.contains("from-backup"), "第 {i} 次：{t}");
            assert_eq!(count_502(), i, "第 {i} 次應該撞到死來源一次");
        }

        // 第四次：死來源已達門檻（連續失敗 3 次）→ 排到最後，備援來源先被試到，
        // 因此**不該再產生任何 502**。
        let before = count_502();
        let r = call().send().await.unwrap();
        assert_eq!(r.status(), 200);
        let t = r.text().await.unwrap();
        assert!(t.contains("from-backup"), "{t}");
        assert_eq!(
            count_502(),
            before,
            "來源已跳開，不該再去撞它（原本 {before} 筆，現在 {} 筆）",
            count_502()
        );
        gw.abort();
    }

    /// 回歸：換協議撞到 404 **不可以**被學成「這個協議會通」。
    ///
    /// 2026-10-07 的真實故障：NIM（宣告 openai-chat）的 `moonshotai/kimi-k3`
    /// 與 `z-ai/glm-5.3` 跑超過 300 秒、NIM 自己回 504 → 網關換手到
    /// `openai-responses` 去打 NIM 根本沒有的 `/responses` → 404。
    ///
    /// 舊版把任何非 400 的回應都當 `SendOutcome::Ok`，於是那個 404 被寫進
    /// `provider_model_protocol`，下一輪被 `plan_attempts` 排到第一位 ——
    /// **永久 404 迴圈**，而且每次失敗都再學一次同樣的錯誤。
    ///
    /// 這裡同時釘住兩件事：
    /// ① 404 不進學習表；
    /// ② 使用者看到的是有意義的 504，而不是 `404 page not found`。
    #[tokio::test]
    async fn e2e_missing_endpoint_is_not_learned_and_504_wins() {
        use axum::response::IntoResponse;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fo-nx.db");
        let (pid, secret) = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            let pid = insert_provider(&conn, "fo-nx-src", "openai-chat", "PLACEHOLDER");
            add_model(&conn, pid, "kimi-test");
            let secret = make_key(&conn, pid, "fo-nx");
            (pid, secret)
        };
        // 假上游：chat 端點像 NIM 一樣「跑太久自己回 504」；responses 端點不存在。
        let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_fake = seen.clone();
        let fake = axum::Router::new().fallback(
            move |uri: axum::http::Uri| {
                let seen = seen_fake.clone();
                async move {
                    seen.lock().unwrap().push(uri.path().to_string());
                    if uri.path().ends_with("/responses") {
                        // go 風格的不存在端點
                        (
                            axum::http::StatusCode::NOT_FOUND,
                            "404 page not found".to_string(),
                        )
                            .into_response()
                    } else {
                        (
                            axum::http::StatusCode::GATEWAY_TIMEOUT,
                            "upstream timeout".to_string(),
                        )
                            .into_response()
                    }
                }
            },
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='fo-nx-src'",
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
            .json(&serde_json::json!({"model": "kimi-test", "input": "hi", "stream": false}))
            .send()
            .await
            .unwrap();

        assert_eq!(
            r.status(),
            504,
            "應該把有意義的 504 回給使用者，而不是換協議撞到的 404"
        );

        // 兩個候選都試過了（chat 逾時 → responses 不存在）
        let seq = seen.lock().unwrap().clone();
        assert!(
            seq.iter().any(|p| p.ends_with("/chat/completions")),
            "應試過 chat：{seq:?}"
        );
        assert!(
            seq.iter().any(|p| p.ends_with("/responses")),
            "應試過 responses：{seq:?}"
        );

        // ★ 核心斷言：404 絕對不可以被學成「這個協議會通」
        let conn = open_conn(&db_path).unwrap();
        let learned: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_model_protocol WHERE provider_id=?1",
                [pid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            learned, 0,
            "上游 404（端點不存在）不該進學習表 —— 否則會變成永久 404 迴圈"
        );
        gw.abort();
    }
