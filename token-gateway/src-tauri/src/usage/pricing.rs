//! 全域定價 CRUD（`pricing` 表）。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- 定價 ---

#[derive(Debug, Clone, Serialize)]
pub struct Pricing {
    pub model_norm: String,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PricingInput {
    pub model_norm: String,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
}

fn row_to_pricing(r: &Row) -> rusqlite::Result<Pricing> {
    Ok(Pricing {
        model_norm: r.get(0)?,
        in_pm: r.get(1)?,
        out_pm: r.get(2)?,
        cache_read_pm: r.get(3)?,
        cache_create_pm: r.get(4)?,
        source: r.get(5)?,
    })
}

pub(crate) fn pricing_list(conn: &Connection) -> rusqlite::Result<Vec<Pricing>> {
    let mut stmt = conn.prepare(
        "SELECT model_norm, in_pm, out_pm, cache_read_pm, cache_create_pm, source
         FROM pricing ORDER BY model_norm ASC",
    )?;
    let rows: Vec<Pricing> = stmt
        .query_map([], row_to_pricing)?
        .collect::<rusqlite::Result<Vec<Pricing>>>()?;
    Ok(rows)
}

pub(crate) fn pricing_upsert(
    conn: &Connection,
    input: &PricingInput,
) -> Result<(), String> {
    let norm = input.model_norm.trim().to_lowercase();
    if norm.is_empty() {
        return Err("模型 ID 不能為空".to_string());
    }
    for (k, v) in [
        ("輸入", input.in_pm),
        ("輸出", input.out_pm),
        ("快取讀取", input.cache_read_pm),
        ("快取創建", input.cache_create_pm),
    ] {
        if !v.is_finite() || v < 0.0 {
            return Err(format!("{k}價格必須是 ≥ 0 的數字"));
        }
    }
    conn.execute(
        "INSERT INTO pricing (model_norm, in_pm, out_pm, cache_read_pm, cache_create_pm, source)
         VALUES (?1, ?2, ?3, ?4, ?5, 'custom')
         ON CONFLICT(model_norm) DO UPDATE SET in_pm=excluded.in_pm, out_pm=excluded.out_pm,
         cache_read_pm=excluded.cache_read_pm, cache_create_pm=excluded.cache_create_pm, source='custom'",
        rusqlite::params![
            norm,
            input.in_pm,
            input.out_pm,
            input.cache_read_pm,
            input.cache_create_pm
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn pricing_delete(conn: &Connection, model_norm: &str) -> Result<(), String> {
    let n = conn
        .execute(
            "DELETE FROM pricing WHERE model_norm = ?1",
            [model_norm.trim().to_lowercase()],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("定價不存在：{model_norm}"));
    }
    Ok(())
}
