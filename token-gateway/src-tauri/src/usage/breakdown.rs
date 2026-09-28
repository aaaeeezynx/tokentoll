//! 維度拆分（by_provider / by_model / by_app）。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;
use serde::Serialize;

use super::{hit_rate, where_sql, UsageFilter};

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub provider_id: Option<i64>,
    pub provider_name: String,
    pub requests: i64,
    pub ok_requests: i64,
    pub success_rate: f64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub tokens: i64,
    /// 快取命中率，與 `UsageSummary` 同一算法（`hit_rate`）。
    pub cache_hit_rate: f64,
    pub cost_usd: f64,
}

pub(crate) fn by_provider(
    conn: &Connection,
    f: &UsageFilter,
) -> rusqlite::Result<Vec<ProviderStat>> {
    let (w, args) = where_sql(f);
    // 注意：where 欄位需加表前綴 l.，因此重寫條件前綴
    let w = w
        .replace("ts >=", "l.ts >=")
        .replace("ts <=", "l.ts <=")
        .replace("app =", "l.app =")
        .replace("provider_id =", "l.provider_id =")
        .replace("model_raw LIKE", "l.model_raw LIKE");
    let mut stmt = conn.prepare(&format!(
        "SELECT l.provider_id, COALESCE(p.name, '(未知渠道)'), COUNT(*),
         COALESCE(SUM(l.in_tok),0), COALESCE(SUM(l.out_tok),0),
         COALESCE(SUM(l.cache_read),0), COALESCE(SUM(l.cache_write),0),
         COALESCE(SUM(l.in_tok+l.out_tok+l.cache_read+l.cache_write),0),
         COALESCE(SUM(l.cost_usd),0),
         COALESCE(SUM(CASE WHEN l.status BETWEEN 200 AND 299 THEN 1 ELSE 0 END),0)
         FROM request_logs l LEFT JOIN providers p ON p.id = l.provider_id
         WHERE {w} GROUP BY l.provider_id ORDER BY 8 DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let requests: i64 = r.get(2)?;
        let ok: i64 = r.get(9)?;
        let in_tok: i64 = r.get(3)?;
        let cache_read: i64 = r.get(5)?;
        Ok(ProviderStat {
            provider_id: r.get(0)?,
            provider_name: r.get(1)?,
            requests,
            ok_requests: ok,
            success_rate: if requests > 0 {
                ok as f64 / requests as f64
            } else {
                0.0
            },
            in_tok,
            out_tok: r.get(4)?,
            cache_read,
            cache_write: r.get(6)?,
            tokens: r.get(7)?,
            cache_hit_rate: hit_rate(in_tok, cache_read),
            cost_usd: r.get(8)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStat {
    pub model: String,
    pub requests: i64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub tokens: i64,
    /// 快取命中率，與 `UsageSummary` 同一算法（`hit_rate`）。
    pub cache_hit_rate: f64,
    pub cost_usd: f64,
}

pub(crate) fn by_model(
    conn: &Connection,
    f: &UsageFilter,
) -> rusqlite::Result<Vec<ModelStat>> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(NULLIF(model_raw,''),'(未知模型)'), COUNT(*),
         COALESCE(SUM(in_tok),0), COALESCE(SUM(out_tok),0),
         COALESCE(SUM(cache_read),0), COALESCE(SUM(cache_write),0),
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w} GROUP BY 1 ORDER BY 7 DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let in_tok: i64 = r.get(2)?;
        let cache_read: i64 = r.get(4)?;
        Ok(ModelStat {
            model: r.get(0)?,
            requests: r.get(1)?,
            in_tok,
            out_tok: r.get(3)?,
            cache_read,
            cache_write: r.get(5)?,
            tokens: r.get(6)?,
            cache_hit_rate: hit_rate(in_tok, cache_read),
            cost_usd: r.get(7)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStat {
    pub app: String,
    pub requests: i64,
    pub ok_requests: i64,
    pub success_rate: f64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub tokens: i64,
    /// 快取命中率，與 `UsageSummary` 同一算法（`hit_rate`）。
    pub cache_hit_rate: f64,
    pub cost_usd: f64,
}

/// 分本機工具統計（「工具」視角用）。
///
/// 這是唯一能**並排比較各工具**的來源：`summary` 一次只回一個工具的聚合，
/// `trend_by_app` 只有 tokens 沒有快取拆分。沿用 `where_sql(f)`，因此範圍／工具／
/// 來源／模型四種篩選自動生效，口徑與其他視角完全一致。
pub(crate) fn by_app(conn: &Connection, f: &UsageFilter) -> rusqlite::Result<Vec<AppStat>> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE(NULLIF(app,''),'unknown'), COUNT(*),
         COALESCE(SUM(in_tok),0), COALESCE(SUM(out_tok),0),
         COALESCE(SUM(cache_read),0), COALESCE(SUM(cache_write),0),
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0),
         COALESCE(SUM(cost_usd),0),
         COALESCE(SUM(CASE WHEN status BETWEEN 200 AND 299 THEN 1 ELSE 0 END),0)
         FROM request_logs WHERE {w} GROUP BY 1 ORDER BY 7 DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let requests: i64 = r.get(1)?;
        let ok: i64 = r.get(8)?;
        let in_tok: i64 = r.get(2)?;
        let cache_read: i64 = r.get(4)?;
        Ok(AppStat {
            app: r.get(0)?,
            requests,
            ok_requests: ok,
            success_rate: if requests > 0 {
                ok as f64 / requests as f64
            } else {
                0.0
            },
            in_tok,
            out_tok: r.get(3)?,
            cache_read,
            cache_write: r.get(5)?,
            tokens: r.get(6)?,
            cache_hit_rate: hit_rate(in_tok, cache_read),
            cost_usd: r.get(7)?,
        })
    })?;
    rows.collect()
}
