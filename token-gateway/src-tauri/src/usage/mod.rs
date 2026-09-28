//! M4：用量聚合查詢 + 定價管理。
//!
//! 時間戳統一用毫秒（`request_logs.ts`）。過濾條件全部可選，前端按
//! 今日 / 7 天 / 30 天換算 `start_ts` 後傳入。

//! 檔案佈局（純重構；對外路徑 `crate::usage::*` 完全不變）：
//! - `stats`：總覽與趨勢聚合（summary / trend / trend_by_app / hourly）
//! - `breakdown`：維度拆分（by_provider / by_model / by_app）
//! - `export`：CSV 匯出
//! - `pricing`：全域定價 CRUD
//! - `provider_pricing`：渠道定價與時段 CRUD
//! - `rates`：費率決議（渠道精確 → 渠道 `*` → 全域 → 0）
//! - `quote`：試算報價與訂閱月費
//! - `tests`：單測（再依主題拆成子模組）
//!
//! 跨子模組共用的私有 helper（`where_sql` / `hit_rate` / `month_usage`）留在本檔。

use rusqlite::Connection;
use serde::Deserialize;

mod breakdown;
mod export;
mod pricing;
mod provider_pricing;
mod quote;
mod rates;
mod stats;

#[cfg(test)]
mod tests;

pub(crate) use breakdown::*;
pub(crate) use export::*;
pub(crate) use pricing::*;
pub(crate) use provider_pricing::*;
pub(crate) use quote::*;
pub(crate) use rates::*;
pub(crate) use stats::*;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UsageFilter {
    pub start_ts: i64,
    pub end_ts: Option<i64>,
    pub app: Option<String>,
    pub provider_id: Option<i64>,
    pub model: Option<String>,
}

fn where_sql(f: &UsageFilter) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;
    let mut conds = vec!["ts >= ?".to_string()];
    let mut args: Vec<Value> = vec![Value::from(f.start_ts)];
    if let Some(e) = f.end_ts {
        conds.push("ts <= ?".to_string());
        args.push(Value::from(e));
    }
    if let Some(a) = f.app.as_deref().filter(|s| !s.is_empty()) {
        conds.push("app = ?".to_string());
        args.push(Value::from(a.to_string()));
    }
    if let Some(p) = f.provider_id {
        conds.push("provider_id = ?".to_string());
        args.push(Value::from(p));
    }
    if let Some(m) = f.model.as_deref().filter(|s| !s.is_empty()) {
        conds.push("model_raw LIKE ?".to_string());
        args.push(Value::from(format!("%{m}%")));
    }
    (conds.join(" AND "), args)
}

/// 快取命中率＝`cache_read / (in_tok + cache_read)`。
///
/// **全檔唯一算法**：`summary` / `by_app` / `by_provider` / `by_model` 一律呼叫這裡，
/// 避免各視角各算一套導致數字互相矛盾（前端「工具」「渠道」「模型」三頁並排比對時尤其致命）。
/// 分母為 0 時回 0.0，不產生 NaN。
pub(crate) fn hit_rate(in_tok: i64, cache_read: i64) -> f64 {
    let denom = in_tok + cache_read;
    if denom > 0 {
        cache_read as f64 / denom as f64
    } else {
        0.0
    }
}

/// 自然月起始（本地時區，毫秒）。
fn month_start_ms(ts_ms: i64) -> i64 {
    use chrono::{Datelike, TimeZone};
    let dt = chrono::Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(chrono::Local::now);
    dt.date_naive()
        .with_day(1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|d| d.and_local_timezone(chrono::Local).single())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// 某渠道（+模型，`*` 表示全渠道）本自然月已用總 tokens。
fn month_usage(
    conn: &Connection,
    provider_id: i64,
    model_norm: &str,
    ts_ms: i64,
) -> rusqlite::Result<i64> {
    let start = month_start_ms(ts_ms);
    if model_norm == "*" {
        conn.query_row(
            "SELECT COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0) FROM request_logs
             WHERE provider_id = ?1 AND ts >= ?2",
            rusqlite::params![provider_id, start],
            |r| r.get(0),
        )
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0) FROM request_logs
             WHERE provider_id = ?1 AND model_norm = ?2 AND ts >= ?3",
            rusqlite::params![provider_id, model_norm, start],
            |r| r.get(0),
        )
    }
}
