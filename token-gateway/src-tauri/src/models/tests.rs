//! 原 `models.rs` 的單元測試；除本檔頭外一字未改。

    use super::*;

    #[test]
    fn parse_both_shapes() {
        let openai: serde_json::Value = serde_json::from_str(
            r#"{"object":"list","data":[{"id":"a"},{"id":"b"},{"id":"a"},{"id":""}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_catalog(&openai, "openai-chat").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        let gem: serde_json::Value = serde_json::from_str(
            r#"{"models":[{"name":"models/gemini-3.6-flash"},{"name":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_catalog(&gem, "gemini").unwrap(),
            vec!["gemini-3.6-flash".to_string(), "x".to_string()]
        );
        assert!(parse_catalog(&openai, "openai-chat").is_ok());
        let bad: serde_json::Value = serde_json::from_str(r#"{"foo":[]}"#).unwrap();
        assert!(parse_catalog(&bad, "openai-chat").is_err());
    }

    #[test]
    fn empty_key_fails_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 種子渠道 key 為空：應直接提示填寫，不發請求
        let err = catalog_fetch(&conn, pid, None, None).unwrap_err();
        assert!(
            err.contains("API Key"),
            "應提示填寫 Key，實際：{err}"
        );
    }

    #[test]
    fn resolve_actual_prefers_display() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "claude-sonnet-4-6".into(),
                actual_model: "deepseek-v4-flash".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(
            resolve_actual(&conn, pid, "claude-sonnet-4-6"),
            "deepseek-v4-flash"
        );
        // actual 本身也命中
        assert_eq!(
            resolve_actual(&conn, pid, "deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
        // 無映射原樣透傳
        assert_eq!(resolve_actual(&conn, pid, "gpt-9"), "gpt-9");
        assert_eq!(resolve_actual(&conn, pid, "  "), "");
    }

    #[test]
    fn crud_validation() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 種子 models_json 已遷移為行
        let rows = models_list(&conn, pid).unwrap();
        assert!(rows.iter().any(|r| r.actual_model == "deepseek-v4-flash"));
        let base = UsableModelInput {
            provider_id: pid,
            display_name: "flash".into(),
            actual_model: "deepseek-v4-flash".into(),
            context_window: Some(128000),
            reasoning: "high".into(),
            ord: 0,
            enabled: true,
        };
        let id = model_add(&conn, &base).unwrap();
        // 顯示名重複被拒
        assert!(model_add(&conn, &base).is_err());
        let mut bad = base.clone();
        bad.reasoning = "turbo".into();
        assert!(model_add(&conn, &bad).is_err());
        bad = base.clone();
        bad.context_window = Some(-1);
        assert!(model_add(&conn, &bad).is_err());
        // 逗號分隔多選推理等級
        let mut multi = base.clone();
        multi.display_name = "multi-reason".into();
        multi.reasoning = "low,medium,high".into();
        let multi_id = model_add(&conn, &multi).unwrap();
        let saved = models_list(&conn, pid)
            .unwrap()
            .into_iter()
            .find(|m| m.id == multi_id)
            .unwrap();
        assert_eq!(saved.reasoning, "low,medium,high");
        // Max 高思考等級可選
        let mut hi = base.clone();
        hi.display_name = "hi-reason".into();
        hi.reasoning = "max".into();
        let hi_id = model_add(&conn, &hi).unwrap();
        model_delete(&conn, hi_id).unwrap();
        // 含無效值的多選被拒
        let mut bad_multi = base.clone();
        bad_multi.display_name = "bad-multi".into();
        bad_multi.reasoning = "low,turbo".into();
        assert!(model_add(&conn, &bad_multi).is_err());
        model_delete(&conn, multi_id).unwrap();
        // 更新 + 刪除
        let mut upd = base.clone();
        upd.display_name = "flash2".into();
        model_update(&conn, id, &upd).unwrap();
        assert!(model_update(&conn, 999999, &upd).is_err());
        model_delete(&conn, id).unwrap();
        assert!(model_delete(&conn, id).is_err());
    }

    #[test]
    fn template_clone_generation() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        // 專用渠道（避開種子遷移行的干擾）
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
             models_json, priority, enabled, created_at, updated_at)
             VALUES ('t6','universal','openai-chat','http://x','', 'bearer','[]',0,1,0,0)",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='t6'", [], |r| r.get(0))
            .unwrap();
        // 自造模板（僅本單測用；真機用用户現有 catalog）
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": [{"slug": "tpl", "display_name": "tpl", "description": "tpl",
                "context_window": 1, "max_context_window": 1, "default_reasoning_level": "low",
                "supports_parallel_tool_calls": true}]}"#,
        )
        .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "我的 flash".into(),
                actual_model: "deepseek-v4-flash".into(),
                context_window: Some(200000),
                reasoning: "high".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        // 禁用的不進入目錄
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "關閉的".into(),
                actual_model: "x".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 10,
                enabled: false,
            },
        )
        .unwrap();
        // 多選推理等級：取規範序第一個作為 Codex 預設
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "multi-reason".into(),
                actual_model: "multi-model".into(),
                context_window: Some(100000),
                reasoning: "minimal,low,medium".into(),
                ord: 5,
                enabled: true,
            },
        )
        .unwrap();
        // 未指定等級 = 全部預設檔位
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "全部檔位".into(),
                actual_model: "all-model".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 3,
                enabled: true,
            },
        )
        .unwrap();
        let dest =
            codex_catalog_generate(&conn, &home, &dir.path().join("data"), pid, true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        let arr = v["models"].as_array().unwrap();
        // 聯合目錄：當前來源 3 個 + 種子渠道啟用模型（zen 等）
        assert!(arr.len() > 3, "應包含其他啟用來源的模型");
        assert!(
            arr.iter().any(|m| m["slug"] == "muse-spark-1.3-contributor-free"),
            "種子 zen 的模型應進入聯合目錄"
        );
        // slug 去重：flatkey 種子也有 deepseek-v4-flash，當前來源優先
        assert_eq!(
            arr.iter().filter(|m| m["slug"] == "deepseek-v4-flash").count(),
            1
        );
        let high_model = arr.iter().find(|m| m["slug"] == "deepseek-v4-flash").unwrap();
        assert_eq!(high_model["display_name"], "我的 flash");
        assert_eq!(high_model["context_window"], 200000);
        assert_eq!(high_model["max_context_window"], 200000);
        assert_eq!(high_model["default_reasoning_level"], "high");
        let efforts = |m: &serde_json::Value| -> Vec<String> {
            m["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|lv| lv["effort"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(efforts(high_model), vec!["high".to_string()]);
        let multi_model = arr.iter().find(|m| m["slug"] == "multi-model").unwrap();
        assert_eq!(multi_model["default_reasoning_level"], "minimal");
        assert_eq!(multi_model["context_window"], 100000);
        assert_eq!(
            efforts(multi_model),
            vec![
                "minimal".to_string(),
                "low".to_string(),
                "medium".to_string()
            ]
        );
        // unset = 模板並集；模板無檔位信息時退標準五檔。
        // 上下文視窗則**不沿用模板值**：沒填就寫 256K（模板寫死 1M，繼承它會讓
        // Codex 遲不壓縮而把請求堆爆上游 —— 見 DEFAULT_CONTEXT_WINDOW 註解）。
        let all_model = arr.iter().find(|m| m["slug"] == "all-model").unwrap();
        assert_eq!(all_model["default_reasoning_level"], "low");
        assert_eq!(
            all_model["context_window"],
            crate::models::DEFAULT_CONTEXT_WINDOW,
            "沒填 context_window 時應寫 256K，不可繼承模板的 1M"
        );
        assert_eq!(
            all_model["max_context_window"],
            crate::models::DEFAULT_CONTEXT_WINDOW
        );
        assert_ne!(all_model["context_window"], 1, "不得殘留模板的極小值");
        assert_ne!(all_model["context_window"], 1000000, "不得繼承模板的 1M");
        assert_eq!(
            efforts(all_model),
            vec![
                "minimal".to_string(),
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "xhigh".to_string()
            ]
        );
        // 禁用模型不進目錄
        assert!(arr.iter().find(|m| m["slug"] == "x").is_none());
        // 模板其餘欄位原樣保留（版本安全）
        assert_eq!(arr[0]["supports_parallel_tool_calls"], true);
        assert_eq!(arr[0]["slug"], "deepseek-v4-flash");
        // issue #3668：桌面版認 `model` 欄位，必須與 slug 一致
        assert!(
            arr.iter().all(|m| m["model"] == m["slug"]),
            "每條目 model 必須等於 slug"
        );
        // 模板收編：legacy 模板應被複製為網關自持副本
        assert!(dir.path().join("data").join("catalogs").join("codex-template.json").exists());
    }

    #[test]
    fn catalog_single_source_excludes_others() {
        // 單來源模式：僅當前來源模型，不含其他啟用來源（選擇器不被污染）
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
             models_json, priority, enabled, created_at, updated_at)
             VALUES ('t7','universal','openai-chat','http://x','', 'bearer','[]',0,1,0,0)",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='t7'", [], |r| r.get(0))
            .unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": [{"slug": "tpl", "display_name": "tpl", "description": "tpl",
                "context_window": 1, "max_context_window": 1, "default_reasoning_level": "low",
                "supports_parallel_tool_calls": true}]}"#,
        )
        .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "only".into(),
                actual_model: "only-model".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        let dest =
            codex_catalog_generate(&conn, &home, &dir.path().join("data"), pid, false).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        let arr = v["models"].as_array().unwrap();
        assert_eq!(arr.len(), 1, "單來源只能有 1 條：{arr:?}");
        assert_eq!(arr[0]["slug"], "only-model");
        assert_eq!(arr[0]["model"], "only-model");
    }

    #[test]
    fn template_vendoring_prefers_gateway_copy() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": []}"#,
        )
        .unwrap();
        let data = dir.path().join("data");
        // 首次：從 legacy 收編
        let p1 = ensure_template(&home, &data).unwrap();
        assert_eq!(p1, data.join("catalogs").join("codex-template.json"));
        assert!(p1.exists());
        // 刪除 legacy 後仍可用網關自持副本（不再依賴 cc-switch）
        std::fs::remove_file(home.join(".codex").join("cc-switch-model-catalog.json")).unwrap();
        let p2 = ensure_template_with(&home, &data, &|| {
            panic!("已有自持副本，不該再去叫 Codex")
        })
        .unwrap();
        assert_eq!(p2, p1);
    }

    /// 兩邊都沒有時**自己跟 Codex 要**（2026-10-08 補的自癒路徑）。
    ///
    /// 這是「第一次安裝、從沒裝過 cc-switch」那台機器的情境：以前必定失敗，
    /// 而錯誤訊息叫他跑的 `codex debug models --bundled` 只印到 stdout、不寫檔。
    #[test]
    fn template_falls_back_to_codex_export_when_nothing_exists() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let data = dir.path().join("data");
        let exported = br#"{"models":[{"slug":"gpt-x","display_name":"GPT-X",
            "context_window":272000,"default_reasoning_level":"low",
            "supported_reasoning_levels":[{"effort":"low","description":"d"}]}]}"#;

        let p = ensure_template_with(&home, &data, &|| Ok(exported.to_vec())).unwrap();
        assert_eq!(p, data.join("catalogs").join("codex-template.json"));
        assert_eq!(
            std::fs::read(&p).unwrap(),
            exported.to_vec(),
            "匯出的內容要原樣落地（不可加 BOM、不可改編碼）"
        );
        // 落地後第二次不該再叫 Codex
        let p2 = ensure_template_with(&home, &data, &|| panic!("第二次不該再叫 Codex")).unwrap();
        assert_eq!(p2, p);
    }

    /// Codex 拿不到時，錯誤訊息必須同時給出兩條可行解法（而不是叫使用者跑一個
    /// 不會產生檔案的指令）。
    #[test]
    fn template_error_gives_actionable_alternatives() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let data = dir.path().join("data");
        let err = ensure_template_with(&home, &data, &|| {
            Err("PATH 上找不到 codex.exe／codex.cmd".to_string())
        })
        .unwrap_err();
        assert!(err.contains("解法一"), "缺少解法一：{err}");
        assert!(err.contains("解法二"), "缺少解法二：{err}");
        assert!(err.contains("同時產生 Codex 模型目錄"), "缺少取消勾選的指示：{err}");
        assert!(!p_of(&data).exists(), "失敗時不該留下半成品模板");
    }

    fn p_of(data: &std::path::Path) -> std::path::PathBuf {
        data.join("catalogs").join("codex-template.json")
    }

    /// 模板內容驗證：擋掉「不是 JSON」「沒有 models」「models 是空的」。
    #[test]
    fn template_validation_rejects_junk() {
        assert!(validate_template(r#"{"models":[{"slug":"a"}]}"#).is_ok());
        assert!(validate_template("not json").is_err());
        assert!(validate_template(r#"{"foo":1}"#).is_err());
        assert!(validate_template(r#"{"models":[]}"#).is_err());
        // 帶 BOM 會解析失敗 —— 所以 `export_template_from_codex` 必須**先**去 BOM
        // 再驗證。真實踩過：PowerShell 的 `>` 會寫出 UTF-16，Set-Content 會寫 BOM。
        assert!(
            validate_template("\u{feff}{\"models\":[{\"slug\":\"a\"}]}").is_err(),
            "serde_json 不接受 BOM，驗證前必須先 strip"
        );
    }

    /// 真機驗證：本機的 Codex CLI 真的找得到、匯出內容也真的能當模板。
    ///
    /// 預設忽略（需要機器上裝了 Codex CLI／桌面版）：
    /// `cargo test --offline --lib -- --ignored live_codex_template_export --nocapture`
    #[test]
    #[ignore = "live: 需要本機安裝 Codex CLI 或桌面版"]
    fn live_codex_template_export() {
        let bytes = export_template_from_codex().expect("應該找得到 Codex 並匯出成功");
        let text = String::from_utf8(bytes).expect("匯出必須是合法 UTF-8");
        assert!(
            !text.starts_with('\u{feff}'),
            "匯出內容不可帶 BOM（serde_json 不接受）"
        );
        validate_template(&text).expect("匯出內容必須是可用模板");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let n = v["models"].as_array().unwrap().len();
        assert!(n > 0, "至少要有一個模型");
        println!("✅ Codex 匯出 {n} 個模型、{} bytes，可當模板", text.len());
    }

    /// 真機驗證（本次修的核心情境）：**完全沒有模板**的乾淨機器能自癒。
    ///
    /// 模擬「第一次安裝 Codex 與本 App、從沒裝過 cc-switch」那台機器：
    /// temp home 沒有 cc-switch 遺留檔、temp app_data 沒有自持副本
    /// —— 舊版在這裡直接失敗，錯誤訊息還叫他跑一個不會產生檔案的指令。
    ///
    /// `cargo test --offline --lib -- --ignored live_ensure_template_self_heals --nocapture`
    #[test]
    #[ignore = "live: 需要本機安裝 Codex CLI 或桌面版"]
    fn live_ensure_template_self_heals_on_a_clean_machine() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let data = dir.path().join("data");
        assert!(
            !home.join(".codex").join("cc-switch-model-catalog.json").exists(),
            "前置條件：這台機器沒有 cc-switch 遺留檔"
        );
        assert!(!p_of(&data).exists(), "前置條件：沒有自持副本");

        let p = ensure_template(&home, &data).expect("乾淨機器上應該能自己跟 Codex 要到模板");
        assert_eq!(p, p_of(&data));
        assert!(p.exists());
        let text = std::fs::read_to_string(&p).unwrap();
        validate_template(&text).expect("自癒產生的模板必須可用");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        println!(
            "✅ 乾淨機器自癒成功：{} 個模型、{} bytes",
            v["models"].as_array().unwrap().len(),
            text.len()
        );
    }
