//! 渠道定價（`provider_pricing`）與時段（`pricing_periods`）CRUD。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::month_usage;

// ---------------------------------------------------------------- 渠道定價 ---
/// 解析後的實際費率（美元/百萬 tokens）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RateSet {
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
}

impl RateSet {
    pub(crate) fn zero() -> Self {
        Self {
            in_pm: 0.0,
            out_pm: 0.0,
            cache_read_pm: 0.0,
            cache_create_pm: 0.0,
        }
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.in_pm == 0.0
            && self.out_pm == 0.0
            && self.cache_read_pm == 0.0
            && self.cache_create_pm == 0.0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPricing {
    pub provider_id: i64,
    pub provider_name: String,
    pub model_norm: String,
    pub mode: String,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
    pub sub_fee_usd: f64,
    pub sub_included_tokens: i64,
    pub source: String,
    /// 訂閱模式本自然月已用（期內總 tokens）；非訂閱恆 0。
    pub used_this_month: i64,
    /// tou 模式該 (provider, model) 的自有時段（不含 `*` 繼承，見 pp_periods）。
    pub periods: Vec<PricingPeriod>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderPricingInput {
    pub provider_id: i64,
    pub model_norm: String,
    pub mode: String,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
    pub sub_fee_usd: f64,
    pub sub_included_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PricingPeriod {
    pub id: i64,
    pub provider_id: i64,
    pub model_norm: String,
    pub name: String,
    pub start: String,
    pub end: String,
    pub ord: i64,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PricingPeriodInput {
    pub provider_id: i64,
    pub model_norm: String,
    pub name: String,
    pub start: String,
    pub end: String,
    pub ord: i64,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
}

fn validate_pp(input: &ProviderPricingInput) -> Result<String, String> {
    if !["usage", "subscription", "tou"].contains(&input.mode.as_str()) {
        return Err("mode 必須是 usage / subscription / tou".to_string());
    }
    let norm = input.model_norm.trim().to_lowercase();
    if norm.is_empty() {
        return Err("模型 ID 不能為空（該渠道預設請填 *）".to_string());
    }
    for (k, v) in [
        ("輸入", input.in_pm),
        ("輸出", input.out_pm),
        ("快取讀取", input.cache_read_pm),
        ("快取創建", input.cache_create_pm),
        ("訂閱費", input.sub_fee_usd),
    ] {
        if !v.is_finite() || v < 0.0 {
            return Err(format!("{k}必須是 ≥ 0 的數字"));
        }
    }
    if input.sub_included_tokens < -1 {
        return Err("訂閱額度必須 ≥ -1（-1 表示無限）".to_string());
    }
    Ok(norm)
}

fn validate_period(input: &PricingPeriodInput) -> Result<String, String> {
    let norm = input.model_norm.trim().to_lowercase();
    if norm.is_empty() {
        return Err("模型 ID 不能為空（該渠道預設請填 *）".to_string());
    }
    if input.name.trim().is_empty() {
        return Err("時段名稱不能為空".to_string());
    }
    for t in [&input.start, &input.end] {
        if chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() {
            return Err(format!("時間格式錯誤（應為 HH:MM）：{t}"));
        }
    }
    for (k, v) in [
        ("輸入", input.in_pm),
        ("輸出", input.out_pm),
        ("快取讀取", input.cache_read_pm),
        ("快取創建", input.cache_create_pm),
    ] {
        if !v.is_finite() || v < 0.0 {
            return Err(format!("{k}必須是 ≥ 0 的數字"));
        }
    }
    Ok(norm)
}

pub(crate) fn pp_list(
    conn: &Connection,
    provider_id: Option<i64>,
    now_ms: i64,
) -> rusqlite::Result<Vec<ProviderPricing>> {
    let mut stmt = conn.prepare(
        "SELECT p.provider_id, COALESCE(pr.name, '(未知渠道)'), p.model_norm, p.mode,
         p.in_pm, p.out_pm, p.cache_read_pm, p.cache_create_pm,
         p.sub_fee_usd, p.sub_included_tokens, p.source
         FROM provider_pricing p LEFT JOIN providers pr ON pr.id = p.provider_id
         WHERE (?1 IS NULL OR p.provider_id = ?1)
         ORDER BY p.provider_id ASC, p.model_norm ASC",
    )?;
    let rows = stmt.query_map([provider_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, f64>(4)?,
            r.get::<_, f64>(5)?,
            r.get::<_, f64>(6)?,
            r.get::<_, f64>(7)?,
            r.get::<_, f64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, String>(10)?,
        ))
    })?;
    let mut out = vec![];
    for row in rows {
        let (pid, pname, model, mode, ip, op, cr, cw, fee, included, source) =
            row?;
        let used = if mode == "subscription" {
            month_usage(conn, pid, &model, now_ms).unwrap_or(0)
        } else {
            0
        };
        let periods = pp_periods(conn, pid, &model).unwrap_or_default();
        out.push(ProviderPricing {
            provider_id: pid,
            provider_name: pname,
            model_norm: model,
            mode,
            in_pm: ip,
            out_pm: op,
            cache_read_pm: cr,
            cache_create_pm: cw,
            sub_fee_usd: fee,
            sub_included_tokens: included,
            source,
            used_this_month: used,
            periods,
        });
    }
    Ok(out)
}

pub(crate) fn pp_upsert(
    conn: &Connection,
    input: &ProviderPricingInput,
) -> Result<(), String> {
    let norm = validate_pp(input)?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM providers WHERE id = ?1",
            [input.provider_id],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return Err(format!("渠道不存在：{}", input.provider_id));
    }
    conn.execute(
        "INSERT INTO provider_pricing (provider_id, model_norm, mode, in_pm, out_pm,
         cache_read_pm, cache_create_pm, sub_fee_usd, sub_included_tokens, source)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'custom')
         ON CONFLICT(provider_id, model_norm) DO UPDATE SET mode=excluded.mode, in_pm=excluded.in_pm,
         out_pm=excluded.out_pm, cache_read_pm=excluded.cache_read_pm, cache_create_pm=excluded.cache_create_pm,
         sub_fee_usd=excluded.sub_fee_usd, sub_included_tokens=excluded.sub_included_tokens,
         source='custom'",
        rusqlite::params![
            input.provider_id,
            norm,
            input.mode,
            input.in_pm,
            input.out_pm,
            input.cache_read_pm,
            input.cache_create_pm,
            input.sub_fee_usd,
            input.sub_included_tokens,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn pp_delete(
    conn: &Connection,
    provider_id: i64,
    model_norm: &str,
) -> Result<(), String> {
    let n = conn
        .execute(
            "DELETE FROM provider_pricing WHERE provider_id = ?1 AND model_norm = ?2",
            rusqlite::params![provider_id, model_norm.trim().to_lowercase()],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("該渠道定價不存在".to_string());
    }
    Ok(())
}

/// 時段 CRUD。
pub(crate) fn pp_periods(
    conn: &Connection,
    provider_id: i64,
    model_norm: &str,
) -> rusqlite::Result<Vec<PricingPeriod>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider_id, model_norm, name, start, end, ord,
         in_pm, out_pm, cache_read_pm, cache_create_pm
         FROM pricing_periods WHERE provider_id = ?1 AND model_norm = ?2
         ORDER BY ord ASC, id ASC",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![provider_id, model_norm.trim().to_lowercase()],
        |r| {
            Ok(PricingPeriod {
                id: r.get(0)?,
                provider_id: r.get(1)?,
                model_norm: r.get(2)?,
                name: r.get(3)?,
                start: r.get(4)?,
                end: r.get(5)?,
                ord: r.get(6)?,
                in_pm: r.get(7)?,
                out_pm: r.get(8)?,
                cache_read_pm: r.get(9)?,
                cache_create_pm: r.get(10)?,
            })
        },
    )?;
    rows.collect()
}

pub(crate) fn pp_period_add(
    conn: &Connection,
    input: &PricingPeriodInput,
) -> Result<i64, String> {
    let norm = validate_period(input)?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM providers WHERE id = ?1",
            [input.provider_id],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return Err(format!("渠道不存在：{}", input.provider_id));
    }
    conn.execute(
        "INSERT INTO pricing_periods (provider_id, model_norm, name, start, end, ord,
         in_pm, out_pm, cache_read_pm, cache_create_pm)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        rusqlite::params![
            input.provider_id,
            norm,
            input.name.trim(),
            input.start,
            input.end,
            input.ord,
            input.in_pm,
            input.out_pm,
            input.cache_read_pm,
            input.cache_create_pm,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub(crate) fn pp_period_update(
    conn: &Connection,
    id: i64,
    input: &PricingPeriodInput,
) -> Result<(), String> {
    let norm = validate_period(input)?;
    let n = conn
        .execute(
            "UPDATE pricing_periods SET model_norm=?1, name=?2, start=?3, end=?4, ord=?5,
             in_pm=?6, out_pm=?7, cache_read_pm=?8, cache_create_pm=?9 WHERE id=?10",
            rusqlite::params![
                norm,
                input.name.trim(),
                input.start,
                input.end,
                input.ord,
                input.in_pm,
                input.out_pm,
                input.cache_read_pm,
                input.cache_create_pm,
                id,
            ],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("時段不存在：{id}"));
    }
    Ok(())
}

pub(crate) fn pp_period_delete(conn: &Connection, id: i64) -> Result<(), String> {
    let n = conn
        .execute("DELETE FROM pricing_periods WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("時段不存在：{id}"));
    }
    Ok(())
}
