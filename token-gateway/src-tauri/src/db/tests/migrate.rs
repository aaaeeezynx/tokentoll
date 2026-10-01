//! 版本升級：既有資料一列不少，最新結構表齊全。
//!
//! 由 `db/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;
use rusqlite::Connection;

    /// v7 → v8 升級：**Phase 1 之前建立的資料庫升級後必須原樣保留既有資料。**
    ///
    /// 這條路徑值得單獨釘住，因為它是**真實使用者第一次啟動新版本時唯一會走
    /// 的路**：`%APPDATA%\com.tokencounter.gateway\app.db` 是長期累積的檔案
    /// （實際測到的現況：v7、7 個 providers、4,111 筆 request_logs）。升級若把
    /// 資料弄丟或讓程式起不來，代價是使用者的全部歷史。
    ///
    /// 作法：造一個「除 Phase 1 那兩張表以外都是最新形狀」的資料庫，再把版本
    /// 退回 7。因為 Phase 1 只**新增**表與索引、沒有動任何既有表的欄位，這與
    /// 真實的 v7 資料庫等價。
    #[test]
    fn v7_db_without_phase1_tables_upgrades_preserving_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            c.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key,
                 models_json, priority, enabled, created_at, updated_at)
                 VALUES ('p','codex','openai-chat','http://x','k','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok,
                 cost_usd, latency_ms, status, is_stream)
                 VALUES (1,'codex','m','m',1,2,0.0,10,200,0)",
                [],
            )
            .unwrap();
            // 退回 v7：拿掉 Phase 1 才有的兩張表與對應版本列
            c.execute_batch(
                "DROP TABLE proxy_trace;
                 DROP TABLE provider_stripped_fields;
                 DELETE FROM schema_version WHERE version >= 8;",
            )
            .unwrap();
            assert!(!has_table(&c, "proxy_trace"), "前置條件：v7 沒有 proxy_trace");
            assert_eq!(max_version(&c), 7, "前置條件：版本應為 7");
        }

        // 比較「升級前 vs 升級後」，而不是比對寫死的數字：`open_and_ensure`
        // 在全新資料庫上會**播種** providers／pricing（見
        // `fresh_db_seeds_providers_and_pricing_once`），所以第一次 `open`
        // 之後 providers 已經不只是上面插入的那一筆。
        let before = {
            let c = Connection::open(&path).unwrap();
            (count_rows(&c, "providers"), count_rows(&c, "request_logs"))
        };
        assert!(before.0 > 0 && before.1 > 0, "前置條件：應有既有資料");

        // 再次開啟 == 使用者啟動新版
        let c = open(&path);
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64, "版本應升到 8");
        assert!(has_table(&c, "proxy_trace"), "升級後應補回 proxy_trace");
        assert!(
            has_table(&c, "provider_stripped_fields"),
            "升級後應補回 provider_stripped_fields"
        );
        assert_eq!(
            count_rows(&c, "providers"),
            before.0,
            "既有 providers 不得遺失"
        );
        assert_eq!(
            count_rows(&c, "request_logs"),
            before.1,
            "既有 request_logs 不得遺失"
        );
    }

    /// 對**指定的真實資料庫副本**跑一次升級，用來在出貨前確認使用者的檔案能
    /// 安全升級、資料不減。
    ///
    /// 先複製再跑，切勿直接指向正式檔：
    /// ```text
    /// copy "%APPDATA%\com.tokencounter.gateway\app.db" "%TEMP%\app_copy.db"
    /// set TOKEN_GATEWAY_MIGRATE_DB=%TEMP%\app_copy.db
    /// cargo test live_migrate_real_db_copy -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn live_migrate_real_db_copy() {
        let Ok(path) = std::env::var("TOKEN_GATEWAY_MIGRATE_DB") else {
            eprintln!("略過：未設定 TOKEN_GATEWAY_MIGRATE_DB");
            return;
        };
        let p = std::path::PathBuf::from(&path);
        let before = {
            let c = Connection::open(&p).expect("開啟副本");
            (
                max_version(&c),
                count_rows(&c, "providers"),
                count_rows(&c, "request_logs"),
            )
        };
        println!(
            "升級前：version={} providers={} request_logs={}",
            before.0, before.1, before.2
        );
        let c = open_and_ensure(&p).expect("升級失敗");
        let after = (
            max_version(&c),
            count_rows(&c, "providers"),
            count_rows(&c, "request_logs"),
            has_table(&c, "proxy_trace"),
            has_table(&c, "provider_stripped_fields"),
        );
        println!(
            "升級後：version={} providers={} request_logs={} proxy_trace={} provider_stripped_fields={}",
            after.0, after.1, after.2, after.3, after.4
        );
        assert_eq!(after.0, SCHEMA_VERSION as i64, "版本應升到最新");
        assert_eq!(after.1, before.1, "providers 不得增減");
        assert_eq!(after.2, before.2, "request_logs 不得增減");
        assert!(after.3 && after.4, "Phase 1 的兩張表應被補上");
    }

    /// **自動化版的 migration 測試（取代只能人工跑的 `live_migrate_real_db_copy`）。**
    ///
    /// 對「每一個歷史版本」各造一個資料庫，逐一確認升級到最新版之後：
    /// 1. 版本升到 [`SCHEMA_VERSION`]
    /// 2. 既有資料一列不少（providers／request_logs／settings）
    /// 3. 最新結構該有的表全部存在
    ///
    /// 為什麼要逐版跑而不是只跑 v7：`open_and_ensure` 是「一次補到最新」，
    /// 真實使用者可能停在**任何**一個舊版，任何一版的落差都會讓他開不起來。
    /// 這個測試讓「每個起點」都被走過一次，且不需人工介入。
    ///
    /// 作法：先建到最新，記下所有表名，再把「比目標版本新的表」與「較新的
    /// schema_version 列」刪掉，即得該版本的等價資料庫（與
    /// `v7_db_without_phase1_tables_upgrades_preserving_data` 同一個手法，
    /// 但對所有版本自動化）。
    #[test]
    fn every_legacy_version_upgrades_preserving_data() {
        // 各版本「新增」的表；升級到該版時應該要有。
        // v8 = Phase 1 的兩張觀測表；其餘版本沒有新增表（改欄位／索引）。
        for target in 1..SCHEMA_VERSION as i64 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("v{target}.db"));
            {
                let c = open(&path);
                // 放進可辨識的既有資料
                c.execute(
                    "INSERT INTO providers (name, app_type, api_format, base_url, api_key,
                     models_json, priority, enabled, created_at, updated_at)
                     VALUES ('keepme','codex','openai-chat','http://x','k','[]',0,1,0,0)",
                    [],
                )
                .unwrap();
                for i in 0..5 {
                    c.execute(
                        "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok,
                         cost_usd, latency_ms, status, is_stream)
                         VALUES (?1,'codex','m','m',1,2,0.0,10,200,0)",
                        [1000 + i],
                    )
                    .unwrap();
                }
                c.execute(
                    "INSERT INTO settings (key, value) VALUES ('probe','v')",
                    [],
                )
                .unwrap();

                // 退回目標版本：刪掉比它新的表與版本列
                if target < 8 {
                    c.execute_batch(
                        "DROP TABLE IF EXISTS proxy_trace;
                         DROP TABLE IF EXISTS provider_stripped_fields;",
                    )
                    .unwrap();
                }
                c.execute(
                    "DELETE FROM schema_version WHERE version > ?1",
                    [target],
                )
                .unwrap();
                assert_eq!(max_version(&c), target, "前置條件：版本應為 {target}");
            }

            let before = {
                let c = Connection::open(&path).unwrap();
                (
                    count_rows(&c, "providers"),
                    count_rows(&c, "request_logs"),
                    count_rows(&c, "settings"),
                )
            };

            // 再次開啟 == 使用者啟動新版
            let c = open(&path);
            assert_eq!(
                max_version(&c),
                SCHEMA_VERSION as i64,
                "v{target} 應升級到 {}",
                SCHEMA_VERSION
            );
            assert_eq!(
                count_rows(&c, "providers"),
                before.0,
                "v{target} 升級後 providers 不得增減"
            );
            assert_eq!(
                count_rows(&c, "request_logs"),
                before.1,
                "v{target} 升級後 request_logs 不得增減"
            );
            // settings 可能被升級流程「補上」新鍵（種子），所以只驗既有鍵沒被動。
            assert!(
                count_rows(&c, "settings") >= before.2,
                "v{target} 升級後 settings 不得減少"
            );
            assert_eq!(
                c.query_row(
                    "SELECT value FROM settings WHERE key='probe'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "v",
                "v{target} 升級後設定值不得變"
            );
            // 最新結構的表必須齊全（與 fresh_db_has_all_core_tables 同一份清單）
            for t in [
                "schema_version",
                "providers",
                "local_keys",
                "request_logs",
                "import_state",
                "pricing",
                "settings",
                "provider_pricing",
                "pricing_periods",
                "provider_models",
                "provider_stripped_fields",
                "provider_model_protocol",
                "provider_usage_query",
                "proxy_trace",
            ] {
                assert!(has_table(&c, t), "v{target} 升級後缺少表 {t}");
            }
            // v14 起六張舊表必須被清掉（MCP／提示詞／技能已移除）
            for t in [
                "mcp_servers",
                "mcp_bindings",
                "prompt_presets",
                "skill_repos",
                "skills",
                "skill_bindings",
            ] {
                assert!(!has_table(&c, t), "v{target} 升級後仍留著 {t}");
            }
        }
    }

    #[test]
    fn open_and_ensure_is_idempotent() {        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let c1 = open(&path);
        assert_eq!(max_version(&c1), SCHEMA_VERSION as i64);
        drop(c1);
        // 再開一次不得報錯、版本不得重複寫入
        let c2 = open(&path);
        assert_eq!(max_version(&c2), SCHEMA_VERSION as i64);
        let rows: i64 = c2
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, SCHEMA_VERSION as i64, "版本列應逐版一列，不重複");
    }

    #[test]
    fn phase1_tables_are_recreated_on_legacy_db() {
        // 模擬 Phase 1 之前的資料庫：兩張新表不存在。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = open(&path);
            c.execute_batch(
                "DROP TABLE proxy_trace; DROP TABLE provider_stripped_fields;",
            )
            .unwrap();
            assert!(!has_table(&c, "proxy_trace"));
        }
        // 重新開啟應自動補回（SCHEMA 的 CREATE TABLE IF NOT EXISTS）
        let c = open(&path);
        assert!(has_table(&c, "proxy_trace"), "proxy_trace 未補回");
        assert!(
            has_table(&c, "provider_stripped_fields"),
            "provider_stripped_fields 未補回"
        );
        // 索引也要在
        let idx: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
                 AND name IN ('idx_proxy_trace_ts','idx_proxy_trace_status')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 2, "proxy_trace 索引未建立");
    }

    /// 開庫時要清掉「指向已不存在來源」的孤兒列，但**不可以**動 `local_keys`。
    ///
    /// 由來（2026-10-01）：實機驗證來源匯出時發現使用者的 `provider_models` 有
    /// 12 列指向 providers 裡已經不存在的 id —— 那是早期刪除來源時還沒有級聯清理
    /// 留下的殘骸。它們讀不到（id 是 AUTOINCREMENT，不會重用），但會讓計數對不上。
    #[test]
    fn open_purges_orphans_but_keeps_local_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orphan.db");
        {
            let c = open_and_ensure(&path).unwrap();
            c.execute("DELETE FROM provider_models", []).unwrap();
            c.execute("DELETE FROM providers", []).unwrap();
            // 孤兒：來源已經不在了
            c.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, ord, enabled)
                 VALUES (999,'ghost','ghost',0,1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO provider_pricing (provider_id, model_norm) VALUES (999,'ghost')",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO pricing_periods (provider_id, model_norm) VALUES (999,'ghost')",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO provider_model_protocol
                 (provider_id, model, declared_format, actual_format, learned_at)
                 VALUES (999,'ghost','openai-chat','openai-responses',1)",
                [],
            )
            .unwrap();
            // 金鑰刻意綁在一個不存在的來源上：這是保留的設計，不可以被清掉
            c.execute(
                "INSERT INTO local_keys (key_prefix, key_hash, key_plain, name, provider_id, created_at)
                 VALUES ('sk-ghost','hash-ghost','','綁到幽靈來源',999,0)",
                [],
            )
            .unwrap();
        }
        let c = open_and_ensure(&path).unwrap();
        for t in [
            "provider_models",
            "provider_pricing",
            "pricing_periods",
            "provider_model_protocol",
            "provider_usage_query",
        ] {
            let n: i64 = c
                .query_row(&format!("SELECT COUNT(*) FROM {t} WHERE provider_id=999"), [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(n, 0, "{t} 的孤兒列應被清掉");
        }
        let keys: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM local_keys WHERE provider_id=999",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(keys, 1, "local_keys 不可被當成孤兒清掉（那是刻意的設計）");
    }

