//! v14 的功能移除：MCP／提示詞／技能三個功能連帶六張表 DROP。
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
