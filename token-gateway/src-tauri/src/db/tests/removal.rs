//! v14／v15 的功能移除：v14 是 MCP／提示詞／技能三個功能連帶六張表 DROP，
//! v15 是 Deep Link 匯入／資料庫備份／更新檢查／雲端同步四個功能（沒有專屬表，
//! 只需清 `settings` 的孤兒列）。
//!
//! 從 `migrate.rs` 分出來，因為那支已經接近 400 行上限，而且這條路徑的性質不同：
//! 一般 migration 是「舊庫補新東西」，這一條是「舊庫**拿掉**東西」——
//! 唯一需要擔心的是刪過頭，所以兩個方向都要釘住。

use super::*;

    /// v13 → v14：MCP／提示詞／技能三個功能移除，六張表要**連資料一起清掉**。
    ///
    /// 這是真實使用者第一次啟動本版時會走的路（他的庫是 v13，實測
    /// `prompt_presets` 有 1 列「現有內容」、`skill_repos` 有 1 列內建儲存庫）。
    /// 兩個方向都要釘住：舊表消失，**其他資料一列不少** —— 使用者已經寫進各工具
    /// 設定檔的內容（`AGENTS.md`、`config.toml` 的 `[mcp_servers]`、各工具
    /// `skills/` 目錄）不在這個庫裡，本來就不受影響。
    #[test]
    fn v13_feature_tables_are_dropped_and_other_data_survives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v13.db");
        let dropped = [
            "mcp_servers",
            "mcp_bindings",
            "prompt_presets",
            "skill_repos",
            "skills",
            "skill_bindings",
        ];
        {
            let c = open(&path);
            // 把 v13 的六張表造回來，並各放一列（模擬真實的 v13 庫）
            c.execute_batch(
                "CREATE TABLE mcp_servers (id INTEGER PRIMARY KEY, slug TEXT);
                 CREATE TABLE mcp_bindings (server_id INTEGER, app TEXT);
                 CREATE TABLE prompt_presets (id INTEGER PRIMARY KEY, app TEXT, name TEXT);
                 CREATE TABLE skill_repos (id INTEGER PRIMARY KEY, owner TEXT);
                 CREATE TABLE skills (id INTEGER PRIMARY KEY, name TEXT);
                 CREATE TABLE skill_bindings (skill_id INTEGER, app TEXT);
                 INSERT INTO mcp_servers (id, slug) VALUES (1,'fetch');
                 INSERT INTO mcp_bindings (server_id, app) VALUES (1,'codex');
                 INSERT INTO prompt_presets (id, app, name) VALUES (1,'codex','現有內容');
                 INSERT INTO skill_repos (id, owner) VALUES (1,'anthropics');
                 INSERT INTO skills (id, name) VALUES (1,'demo');
                 INSERT INTO skill_bindings (skill_id, app) VALUES (1,'claude');
                 DELETE FROM schema_version WHERE version >= 14;",
            )
            .unwrap();
            assert!(has_table(&c, "mcp_servers"), "前置條件：v13 有 mcp_servers");
            assert_eq!(max_version(&c), 13, "前置條件：版本應為 13");
            // 使用者的資料：升級後必須原樣還在
            c.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key,
                 models_json, priority, enabled, created_at, updated_at)
                 VALUES ('keep','codex','openai-chat','http://x','k','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO settings (key, value) VALUES ('keep_me','v')",
                [],
            )
            .unwrap();
        }

        let c = open(&path);
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64, "應升到最新版");
        for t in dropped {
            assert!(!has_table(&c, t), "v14 應清掉 {t}");
        }
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM providers WHERE name='keep'", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1,
            "來源不得受影響"
        );
        assert_eq!(
            c.query_row("SELECT value FROM settings WHERE key='keep_me'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "v",
            "設定不得受影響"
        );
        // 其餘核心表也要還在（避免「不小心刪過頭」）
        for t in ["request_logs", "local_keys", "pricing", "proxy_trace"] {
            assert!(has_table(&c, t), "{t} 不該被動到");
        }
    }

    /// v14 → v15：Deep Link 匯入／資料庫備份／更新檢查／雲端同步四個功能移除。
    ///
    /// 與 v14 相反 —— 這四個功能**沒有專屬資料表**，狀態全在通用的 `settings`
    /// 裡，所以沒有表可以 DROP，只有 17 個孤兒列要清。風險點跟 v14 一樣是
    /// 「刪過頭」：`settings` 裡還有 theme／lang／accent／gateway_port 等
    /// 天天在用的設定，一個 `DELETE FROM settings` 就全沒了。
    ///
    /// 真正非清不可的是 `cloud_password`（**明文 WebDAV 密碼**）：功能拿掉了
    /// 但憑證還躺在 app.db 裡，是實質的資訊留存。
    #[test]
    fn v14_feature_settings_are_purged_and_live_settings_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v14.db");
        let purged_keys = [
            // 雲端同步
            "cloud_provider",
            "cloud_base_url",
            "cloud_username",
            "cloud_password",
            "cloud_remote_root",
            "cloud_profile",
            "cloud_auto_sync",
            "cloud_last_sync_ms",
            "cloud_last_result",
            "cloud_last_ok",
            "cloud_last_mtime",
            "cloud_last_bytes",
            // 資料庫備份
            "db_backup_interval",
            "db_backup_keep",
            // 更新檢查
            "update_manifest_url",
            "update_last_check_ms",
            "update_last_result",
        ];
        // 還在用的設定：theme／lang／accent／close_action 由 SEED 預先寫好，
        // gateway_port 在剛開的新庫裡還不存在 —— 兩種都覆蓋到。
        let live_keys = ["theme", "lang", "accent", "close_action", "gateway_port"];
        let before: Vec<(String, String)> = {
            let c = open(&path);
            for (i, k) in purged_keys.iter().enumerate() {
                c.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                    rusqlite::params![k, format!("stale-{i}")],
                )
                .unwrap();
            }
            let snapshot = live_keys
                .iter()
                .map(|k| {
                    (
                        k.to_string(),
                        c.query_row("SELECT value FROM settings WHERE key=?1", [k], |r| r
                            .get::<_, String>(0))
                            .unwrap_or_default(),
                    )
                })
                .collect();
            c.execute("DELETE FROM schema_version WHERE version >= 15", [])
                .unwrap();
            assert_eq!(max_version(&c), 14, "前置條件：版本應為 14");
            assert!(has_setting(&c, "cloud_password"), "前置條件：應有舊的雲端密碼");
            snapshot
        };

        let c = open(&path);
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64, "應升到最新版");
        for k in purged_keys {
            assert!(
                !has_setting(&c, k),
                "v15 應清掉 {k}（留下就是沒人讀的孤兒列；cloud_password 更是憑證外流）"
            );
        }
        for (k, want) in &before {
            assert_eq!(
                c.query_row("SELECT value FROM settings WHERE key=?1", [k], |r| r
                    .get::<_, String>(0))
                    .unwrap_or_default(),
                *want,
                "{k} 是還在用的設定，不該被刪過頭"
            );
        }
        // 核心資料一列不少
        for t in ["providers", "request_logs", "local_keys", "provider_models"] {
            assert!(has_table(&c, t), "{t} 不該被動到");
        }
    }
