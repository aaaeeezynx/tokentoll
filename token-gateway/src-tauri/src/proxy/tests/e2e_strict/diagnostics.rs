//! 被網關拒絕的請求必須在診斷（`proxy_trace`）查得到的端到端測試。

    use super::super::*;

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
