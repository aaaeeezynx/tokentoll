//! 遷移與種子：v3 峰谷 → 時段的遷移，以及種子只下發一次。
//!
//! 由 `usage/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;
use rusqlite::Connection;

    #[test]
    fn seeds_apply_once_and_stay_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.db");
        let conn = crate::db::open_and_ensure(&path).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert!(n >= 3);
        // 用戶刪除後重啟 ensure，不應復活
        conn.execute("DELETE FROM providers", []).unwrap();
        conn.execute("DELETE FROM pricing", []).unwrap();
        drop(conn);
        let conn2 = crate::db::open_and_ensure(&path).unwrap();
        let n2: i64 = conn2
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        let p2: i64 = conn2
            .query_row("SELECT COUNT(*) FROM pricing", [], |r| r.get(0))
            .unwrap();
        assert_eq!((n2, p2), (0, 0));
    }

    #[test]
    fn v3_tou_migrates_to_periods() {
        // 手工構造 v2 完整形狀（含 peak_* 列）的庫，再跑 ensure 觸發遷移
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
                 CREATE TABLE providers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                   app_type TEXT NOT NULL DEFAULT '', api_format TEXT NOT NULL DEFAULT '',
                   base_url TEXT NOT NULL DEFAULT '', api_key TEXT NOT NULL DEFAULT '',
                   models_json TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 0,
                   enabled INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL DEFAULT 0,
                   updated_at INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE local_keys (id INTEGER PRIMARY KEY AUTOINCREMENT,
                   key_prefix TEXT NOT NULL DEFAULT '', key_hash TEXT NOT NULL UNIQUE,
                   name TEXT NOT NULL DEFAULT '', quota_tokens INTEGER NOT NULL DEFAULT -1,
                   used_tokens INTEGER NOT NULL DEFAULT 0, rate_limit_qpm INTEGER NOT NULL DEFAULT 60,
                   allowed_models_json TEXT NOT NULL DEFAULT '[]', allowed_apps_json TEXT NOT NULL DEFAULT '[]',
                   provider_id INTEGER, expires_at INTEGER, enabled INTEGER NOT NULL DEFAULT 1,
                   created_at INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE request_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL,
                   key_id INTEGER, app TEXT NOT NULL DEFAULT '', provider_id INTEGER,
                   model_raw TEXT NOT NULL DEFAULT '', model_norm TEXT NOT NULL DEFAULT '',
                   in_tok INTEGER NOT NULL DEFAULT 0, out_tok INTEGER NOT NULL DEFAULT 0,
                   cache_read INTEGER NOT NULL DEFAULT 0, cache_write INTEGER NOT NULL DEFAULT 0,
                   cost_usd REAL NOT NULL DEFAULT 0, latency_ms INTEGER NOT NULL DEFAULT 0,
                   status INTEGER NOT NULL DEFAULT 0, is_stream INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE pricing (model_norm TEXT PRIMARY KEY, in_pm REAL NOT NULL DEFAULT 0,
                   out_pm REAL NOT NULL DEFAULT 0, cache_read_pm REAL NOT NULL DEFAULT 0,
                   cache_create_pm REAL NOT NULL DEFAULT 0, source TEXT NOT NULL DEFAULT '');
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '');
                 CREATE TABLE provider_pricing (provider_id INTEGER NOT NULL, model_norm TEXT NOT NULL,
                   mode TEXT NOT NULL DEFAULT 'usage', in_pm REAL NOT NULL DEFAULT 0, out_pm REAL NOT NULL DEFAULT 0,
                   cache_read_pm REAL NOT NULL DEFAULT 0, cache_create_pm REAL NOT NULL DEFAULT 0,
                   sub_fee_usd REAL NOT NULL DEFAULT 0, sub_included_tokens INTEGER NOT NULL DEFAULT -1,
                   peak_start TEXT NOT NULL DEFAULT '09:00', peak_end TEXT NOT NULL DEFAULT '18:00',
                   peak_in_pm REAL, peak_out_pm REAL, peak_cache_read_pm REAL, peak_cache_create_pm REAL,
                   source TEXT NOT NULL DEFAULT 'custom', PRIMARY KEY (provider_id, model_norm));
                 INSERT INTO schema_version VALUES (2);
                 INSERT INTO providers (id, name) VALUES (7, 'old');
                 INSERT INTO provider_pricing VALUES (7, 'm', 'tou', 1.0, 2.0, 0.1, 0.2, 0, -1,
                   '10:00', '12:00', 5.0, NULL, NULL, NULL, 'custom');",
            )
            .unwrap();
        }
        let conn = crate::db::open_and_ensure(&path).unwrap();
        // 遷移出一個時段，NULL 峰價回退基礎
        let ps = pp_periods(&conn, 7, "m").unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!((ps[0].start.as_str(), ps[0].end.as_str()), ("10:00", "12:00"));
        assert!((ps[0].in_pm - 5.0).abs() < 1e-9);
        assert!((ps[0].out_pm - 2.0).abs() < 1e-9);
        // 舊列已刪除
        let has: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('provider_pricing') WHERE name='peak_start'")
            .unwrap()
            .exists([])
            .unwrap();
        assert!(!has);
        // 版本升到當前
        let ver: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, crate::db::SCHEMA_VERSION as i64);
    }
