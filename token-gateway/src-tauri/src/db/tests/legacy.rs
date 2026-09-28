//! 舊庫補列／遷移路徑（ALTER 與 DROP COLUMN）。
//!
//! 由 `db/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;
use rusqlite::Connection;

    #[test]
    fn legacy_local_keys_gains_provider_id_and_key_plain() {
        // v1 → v2（provider_id）與 v6 → v7（key_plain）的 ALTER 路徑。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE local_keys (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    key_prefix TEXT NOT NULL,
                    key_hash TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL DEFAULT '',
                    quota_tokens INTEGER NOT NULL DEFAULT -1,
                    used_tokens INTEGER NOT NULL DEFAULT 0,
                    rate_limit_qpm INTEGER NOT NULL DEFAULT 60,
                    allowed_models_json TEXT NOT NULL DEFAULT '[]',
                    allowed_apps_json TEXT NOT NULL DEFAULT '[]',
                    expires_at INTEGER,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL
                 );
                 INSERT INTO local_keys (key_prefix, key_hash, created_at)
                 VALUES ('sk-local-abc', 'hash1', 1000);",
            )
            .unwrap();
        }
        let c = open(&path);
        assert!(has_column(&c, "local_keys", "provider_id"));
        assert!(has_column(&c, "local_keys", "key_plain"));
        // 既有資料必須保留，新欄位取預設值
        let (prefix, pid, plain, created): (String, Option<i64>, String, i64) = c
            .query_row(
                "SELECT key_prefix, provider_id, key_plain, created_at FROM local_keys",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(prefix, "sk-local-abc");
        assert_eq!(pid, None);
        assert_eq!(plain, "");
        assert_eq!(created, 1000);
    }

    #[test]
    fn legacy_request_logs_gains_source_and_import_path() {
        // v5 → v6（M5 歷史回填）的 ALTER 路徑。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE request_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    ts INTEGER NOT NULL,
                    key_id INTEGER,
                    app TEXT NOT NULL DEFAULT '',
                    provider_id INTEGER,
                    model_raw TEXT NOT NULL DEFAULT '',
                    model_norm TEXT NOT NULL DEFAULT '',
                    in_tok INTEGER NOT NULL DEFAULT 0,
                    out_tok INTEGER NOT NULL DEFAULT 0,
                    cache_read INTEGER NOT NULL DEFAULT 0,
                    cache_write INTEGER NOT NULL DEFAULT 0,
                    cost_usd REAL NOT NULL DEFAULT 0,
                    latency_ms INTEGER NOT NULL DEFAULT 0,
                    status INTEGER NOT NULL DEFAULT 0,
                    is_stream INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT INTO request_logs (ts, app) VALUES (500, 'codex');",
            )
            .unwrap();
        }
        let c = open(&path);
        assert!(has_column(&c, "request_logs", "source"));
        assert!(has_column(&c, "request_logs", "import_path"));
        let (app, source, ip): (String, String, String) = c
            .query_row(
                "SELECT app, source, import_path FROM request_logs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(app, "codex");
        assert_eq!(source, "gateway", "舊列應補上預設來源");
        assert_eq!(ip, "");
        // ALTER 之後才建的索引必須存在
        let idx: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
                 AND name IN ('idx_logs_import','idx_logs_dedupe')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 2, "v6 索引未建立");
    }

    #[test]
    fn legacy_tou_pricing_migrates_to_periods_and_drops_peak_columns() {
        // v3 → v4：單窗口峰谷 → pricing_periods 多時段，然後 DROP 舊列。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE provider_pricing (
                    provider_id INTEGER NOT NULL,
                    model_norm TEXT NOT NULL,
                    mode TEXT NOT NULL DEFAULT 'usage',
                    in_pm REAL NOT NULL DEFAULT 0,
                    out_pm REAL NOT NULL DEFAULT 0,
                    cache_read_pm REAL NOT NULL DEFAULT 0,
                    cache_create_pm REAL NOT NULL DEFAULT 0,
                    peak_start TEXT NOT NULL DEFAULT '',
                    peak_end TEXT NOT NULL DEFAULT '',
                    peak_in_pm REAL,
                    peak_out_pm REAL,
                    peak_cache_read_pm REAL,
                    peak_cache_create_pm REAL
                 );
                 INSERT INTO provider_pricing
                   (provider_id, model_norm, mode, in_pm, out_pm, peak_start, peak_end,
                    peak_in_pm, peak_out_pm, peak_cache_read_pm, peak_cache_create_pm)
                 VALUES (3, '*', 'tou', 1.0, 2.0, '08:00', '20:00', 5.0, 6.0, 0.5, 0.6);
                 -- 非 tou 列不應被遷移
                 INSERT INTO provider_pricing (provider_id, model_norm, mode, in_pm, out_pm, peak_start, peak_end)
                 VALUES (4, 'm', 'usage', 1.0, 2.0, '', '');",
            )
            .unwrap();
        }
        let c = open(&path);
        // 舊欄位已移除
        for col in ["peak_start", "peak_end", "peak_in_pm", "peak_out_pm"] {
            assert!(!has_column(&c, "provider_pricing", col), "{col} 未移除");
        }
        // 峰時段已遷移
        let (name, start, end, ipm, opm, cr, cc): (String, String, String, f64, f64, f64, f64) = c
            .query_row(
                "SELECT name, start, end, in_pm, out_pm, cache_read_pm, cache_create_pm
                 FROM pricing_periods WHERE provider_id=3 AND model_norm='*'",
                [],
                |r| {
                    Ok((
                        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(name, "峰時");
        assert_eq!((start.as_str(), end.as_str()), ("08:00", "20:00"));
        assert_eq!((ipm, opm, cr, cc), (5.0, 6.0, 0.5, 0.6));
        // 非 tou 列不產生時段
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM pricing_periods WHERE provider_id=4",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        // 原始列保留（遷移只補時段，不刪定價列）
        let kept: i64 = c
            .query_row("SELECT COUNT(*) FROM provider_pricing", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 2);
    }
