//! 總覽與趨勢聚合（summary / trend / trend_by_app / hourly）。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;
use serde::Serialize;

use super::{hit_rate, where_sql, UsageFilter};

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub requests: i64,
    pub ok_requests: i64,
    pub success_rate: f64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// 快取歸一化總量（與 cc-switch Hero 口徑一致）。
    pub total_tokens: i64,
    pub cache_hit_rate: f64,
    pub cost_usd: f64,
}

pub(crate) fn summary(
    conn: &Connection,
    f: &UsageFilter,
) -> rusqlite::Result<UsageSummary> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status BETWEEN 200 AND 299 THEN 1 ELSE 0 END),0),
         COALESCE(SUM(in_tok),0), COALESCE(SUM(out_tok),0),
         COALESCE(SUM(cache_read),0), COALESCE(SUM(cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w}"
    ))?;
    let (requests, ok, itok, otok, cr, cw, cost): (i64, i64, i64, i64, i64, i64, f64) =
        stmt.query_row(rusqlite::params_from_iter(args), |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
    let total = itok + otok + cr + cw;
    Ok(UsageSummary {
        requests,
        ok_requests: ok,
        success_rate: if requests > 0 {
            ok as f64 / requests as f64
        } else {
            0.0
        },
        in_tok: itok,
        out_tok: otok,
        cache_read: cr,
        cache_write: cw,
        total_tokens: total,
        cache_hit_rate: hit_rate(itok, cr),
        cost_usd: cost,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendBucket {
    /// 桶起始毫秒時間戳。
    pub ts: i64,
    pub requests: i64,
    pub tokens: i64,
    pub cost_usd: f64,
}

pub(crate) fn trend(
    conn: &Connection,
    f: &UsageFilter,
    bucket_secs: i64,
) -> rusqlite::Result<Vec<TrendBucket>> {
    let bms = bucket_secs.max(60) * 1000;
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT (ts/?1)*?1 AS b, COUNT(*),
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w} GROUP BY b ORDER BY b ASC"
    ))?;
    let mut full_args = vec![rusqlite::types::Value::from(bms)];
    full_args.extend(args);
    let rows = stmt.query_map(rusqlite::params_from_iter(full_args), |r| {
        Ok(TrendBucket {
            ts: r.get(0)?,
            requests: r.get(1)?,
            tokens: r.get(2)?,
            cost_usd: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendAppRow {
    /// 桶起始毫秒時間戳。
    pub ts: i64,
    pub app: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost_usd: f64,
}

/// 趨勢按應用拆分（堆疊柱狀圖用）。
pub(crate) fn trend_by_app(
    conn: &Connection,
    f: &UsageFilter,
    bucket_secs: i64,
) -> rusqlite::Result<Vec<TrendAppRow>> {
    let bms = bucket_secs.max(60) * 1000;
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT (ts/?1)*?1 AS b, COALESCE(NULLIF(app,''),'unknown'), COUNT(*),
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w} GROUP BY b, COALESCE(NULLIF(app,''),'unknown') ORDER BY b ASC"
    ))?;
    let mut full_args = vec![rusqlite::types::Value::from(bms)];
    full_args.extend(args);
    let rows = stmt.query_map(rusqlite::params_from_iter(full_args), |r| {
        Ok(TrendAppRow {
            ts: r.get(0)?,
            app: r.get(1)?,
            requests: r.get(2)?,
            tokens: r.get(3)?,
            cost_usd: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// 按小時聚合（0-23），顯示每日時段活躍度。
#[derive(Debug, Clone, Serialize)]
pub struct HourlyBucket {
    /// 小時 (0-23)。
    pub hour: i64,
    pub requests: i64,
    pub tokens: i64,
    pub cost_usd: f64,
}

pub(crate) fn hourly(
    conn: &Connection,
    f: &UsageFilter,
) -> rusqlite::Result<Vec<HourlyBucket>> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT CAST(strftime('%H', ts / 1000, 'unixepoch', 'localtime') AS INTEGER) AS h,
         COUNT(*),
         COALESCE(SUM(in_tok + out_tok + cache_read + cache_write), 0),
         COALESCE(SUM(cost_usd), 0)
         FROM request_logs WHERE {w} GROUP BY h ORDER BY h ASC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok(HourlyBucket {
            hour: r.get(0)?,
            requests: r.get(1)?,
            tokens: r.get(2)?,
            cost_usd: r.get(3)?,
        })
    })?;
    rows.collect()
}
