//! usage 單測的入口與共用前置（原 `usage.rs` 的 `mod tests`）。
//!
//! 測試內容與斷言一字未改，只依主題拆成子模組。`use super::*;` 會帶進本模組
//! 自己定義的 helper；子模組再看得到這裡的私有項目。

use super::*;
use rusqlite::Connection;

    fn seed_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        // 預設渠道應已種入
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert!(n >= 3, "應種入 3 個預設渠道，實際 {n}");
        let p: f64 = conn
            .query_row(
                "SELECT in_pm FROM pricing WHERE model_norm='deepseek-v4-flash'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!((p - 0.14).abs() < 1e-9);
        (dir, conn)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_log(
        conn: &Connection,
        ts: i64,
        app: &str,
        provider_id: Option<i64>,
        model: &str,
        itok: i64,
        otok: i64,
        cost: f64,
        status: i64,
    ) {
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,?2,?3,?4,?4,?5,?6,?7,10,?8,0)",
            rusqlite::params![ts, app, provider_id, model, itok, otok, cost, status],
        )
        .unwrap();
    }

    fn provider_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM providers WHERE name = ?1", [name], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn pp(
        pid: i64,
        model: &str,
        mode: &str,
    ) -> ProviderPricingInput {
        ProviderPricingInput {
            provider_id: pid,
            model_norm: model.into(),
            mode: mode.into(),
            in_pm: 1.0,
            out_pm: 2.0,
            cache_read_pm: 0.1,
            cache_create_pm: 0.2,
            sub_fee_usd: 20.0,
            sub_included_tokens: 1000,
        }
    }

    fn period(pid: i64, model: &str, name: &str, start: &str, end: &str) -> PricingPeriodInput {
        PricingPeriodInput {
            provider_id: pid,
            model_norm: model.into(),
            name: name.into(),
            start: start.into(),
            end: end.into(),
            ord: 0,
            in_pm: 2.0,
            out_pm: 4.0,
            cache_read_pm: 0.2,
            cache_create_pm: 0.4,
        }
    }

    fn ts_at(hour: u32, min: u32) -> i64 {
        use chrono::Local;
        let today = Local::now().date_naive();
        today
            .and_hms_opt(hour, min, 0)
            .and_then(|d| d.and_local_timezone(Local).single())
            .map(|d| d.timestamp_millis())
            .unwrap()
    }

    /// 插入含快取欄位的日誌。
    #[allow(clippy::too_many_arguments)]
    fn add_log_cache(
        conn: &Connection,
        ts: i64,
        app: &str,
        model: &str,
        itok: i64,
        otok: i64,
        cr: i64,
        cw: i64,
        status: i64,
    ) {
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok,
                                       cache_read, cache_write, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,?2,NULL,?3,?3,?4,?5,?6,?7,0,10,?8,0)",
            rusqlite::params![ts, app, model, itok, otok, cr, cw, status],
        )
        .unwrap();
    }

mod agg;
mod by_app;
mod migrate;
mod prices;
mod resolve;
