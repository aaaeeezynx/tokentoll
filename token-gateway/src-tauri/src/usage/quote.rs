//! 試算報價（price_quote）與訂閱月費匯總。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;
use serde::Serialize;

use super::month_usage;
use super::provider_pricing::RateSet;
use super::rates::{global_rates, resolve_rates, strip_date_tail};

#[derive(Debug, Clone, Serialize)]
pub struct PriceQuote {
    /// 按量＝當次費用；訂閱＝月費＋場景超額費用。
    pub cost_usd: f64,
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
    pub source: String,
    pub mode: String,
    pub sub_fee_usd: f64,
    pub sub_included_tokens: i64,
    pub sub_used_this_month: i64,
    pub sub_over_tokens: i64,
}

/// 命中層級元數據（與 resolve_rates 同順序：渠道精確 → 渠道 `*` → 全域）。
/// 僅「有費率內容」的行算命中（全零行視為無定價，與決議一致）。
struct QuoteMeta {
    source: String,
    mode: String,
    fee: f64,
    included: i64,
    /// 用於查本月已用的 (provider_id, 行 key)：精確行用模型名，`*` 行用 `"*"`。
    used_key: Option<(i64, String)>,
}

fn priced_row_full(
    conn: &Connection,
    table_where: &str,
    params: &[&dyn rusqlite::ToSql],
    want_mode: bool,
) -> Option<(String, f64, i64, f64, f64, f64, f64)> {
    // 渠道表與全域表列不同：渠道行帶 mode/fee/included，全域行只有四率。
    let sql = if want_mode {
        format!("SELECT mode, sub_fee_usd, sub_included_tokens, in_pm, out_pm, cache_read_pm, cache_create_pm FROM {table_where}")
    } else {
        format!("SELECT in_pm, out_pm, cache_read_pm, cache_create_pm FROM {table_where}")
    };
    let r: rusqlite::Result<(String, f64, i64, f64, f64, f64, f64)> =
        conn.query_row(&sql, params, |r| {
            if want_mode {
                Ok((
                    r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?,
                    r.get(5)?, r.get(6)?,
                ))
            } else {
                Ok((
                    "usage".to_string(), 0.0, -1, r.get(0)?, r.get(1)?, r.get(2)?,
                    r.get(3)?,
                ))
            }
        });
    r.ok().filter(|(_, _, _, a, b, c, d)| {
        !(*a == 0.0 && *b == 0.0 && *c == 0.0 && *d == 0.0)
    })
}

fn quote_meta(
    conn: &Connection,
    provider_id: Option<i64>,
    model_norm: &str,
) -> QuoteMeta {
    let blank = |source: String| QuoteMeta {
        source,
        mode: "usage".to_string(),
        fee: 0.0,
        included: -1,
        used_key: None,
    };
    let mut norm = model_norm.trim().to_lowercase();
    let pname: Option<String> = provider_id.and_then(|pid| {
        conn.query_row(
            "SELECT name FROM providers WHERE id = ?1",
            rusqlite::params![pid],
            |r| r.get(0),
        )
        .ok()
    });
    for _ in 0..4 {
        if let Some(pid) = provider_id {
            if let Some((mode, fee, included, _, _, _, _)) = priced_row_full(
                conn,
                "provider_pricing WHERE provider_id = ?1 AND model_norm = ?2",
                &[&pid, &norm],
                true,
            ) {
                return QuoteMeta {
                    source: format!("{}・模型", pname.clone().unwrap_or("來源".into())),
                    mode,
                    fee,
                    included,
                    used_key: Some((pid, norm.clone())),
                };
            }
        }
        match strip_date_tail(&norm) {
            Some(p) => norm = p,
            None => break,
        }
    }
    if let Some(pid) = provider_id {
        if let Some((mode, fee, included, _, _, _, _)) = priced_row_full(
            conn,
            "provider_pricing WHERE provider_id = ?1 AND model_norm = '*'",
            &[&pid],
            true,
        ) {
            return QuoteMeta {
                source: format!("{}・預設", pname.unwrap_or("來源".into())),
                mode,
                fee,
                included,
                used_key: Some((pid, "*".to_string())),
            };
        }
    }
    let mut gnorm = model_norm.trim().to_lowercase();
    for _ in 0..4 {
        if priced_row_full(conn, "pricing WHERE model_norm = ?1", &[&gnorm], false)
            .is_some()
        {
            return blank("全域預設".to_string());
        }
        match strip_date_tail(&gnorm) {
            Some(p) => gnorm = p,
            None => break,
        }
    }
    blank("未定價（記 0）".to_string())
}

/// 訂閱超額分攤：剩餘免費額度按 輸入→輸出→快取讀→快取建 順序抵扣場景量。
fn sub_over_cost(
    included: i64,
    used: i64,
    amounts: [i64; 4],
    rates: [f64; 4],
) -> (f64, i64) {
    if included < 0 {
        return (0.0, 0);
    }
    let mut rem = (included - used).max(0);
    let mut over_cost = 0.0;
    let mut over_tok = 0i64;
    for (i, amt) in amounts.iter().enumerate() {
        let amt = (*amt).max(0);
        let take = rem.min(amt);
        rem -= take;
        let over = amt - take;
        over_tok += over;
        over_cost += over as f64 / 1e6 * rates[i];
    }
    (over_cost, over_tok)
}

/// 試算報價：按決議順序取費率（tou 按當下時段），返回金額＋命中層級。
/// 訂閱模式下 cost_usd＝月費＋場景超額（超額按基礎價，非時段價，與決議一致）。
pub(crate) fn price_quote(
    conn: &Connection,
    provider_id: Option<i64>,
    model_norm: &str,
    in_tok: i64,
    out_tok: i64,
    cache_read: i64,
    cache_write: i64,
) -> rusqlite::Result<PriceQuote> {
    let now = crate::fsutil::now_ms();
    let rates = resolve_rates(conn, provider_id, model_norm, now);
    let meta = quote_meta(conn, provider_id, model_norm);
    let used = meta
        .used_key
        .as_ref()
        .and_then(|(pid, key)| month_usage(conn, *pid, key, now).ok())
        .unwrap_or(0);
    let (cost, over_tok) = if meta.mode == "subscription" {
        // 超額基礎價：取命中行的基礎四率（與決議超額口徑一致）。
        let base = base_rates(conn, provider_id, model_norm);
        let (over, over_tok) = sub_over_cost(
            meta.included,
            used,
            [in_tok, out_tok, cache_read, cache_write],
            [base.in_pm, base.out_pm, base.cache_read_pm, base.cache_create_pm],
        );
        (meta.fee + over, over_tok)
    } else {
        (
            in_tok.max(0) as f64 / 1e6 * rates.in_pm
                + out_tok.max(0) as f64 / 1e6 * rates.out_pm
                + cache_read.max(0) as f64 / 1e6 * rates.cache_read_pm
                + cache_write.max(0) as f64 / 1e6 * rates.cache_create_pm,
            0,
        )
    };
    Ok(PriceQuote {
        cost_usd: cost,
        in_pm: rates.in_pm,
        out_pm: rates.out_pm,
        cache_read_pm: rates.cache_read_pm,
        cache_create_pm: rates.cache_create_pm,
        source: meta.source,
        mode: meta.mode,
        sub_fee_usd: meta.fee,
        sub_included_tokens: meta.included,
        sub_used_this_month: used,
        sub_over_tokens: over_tok,
    })
}

/// 命中行的基礎四率（訂閱超額口徑：忽略免費歸零，與決議同順序）。
fn base_rates(
    conn: &Connection,
    provider_id: Option<i64>,
    model_norm: &str,
) -> RateSet {
    let mut norm = model_norm.trim().to_lowercase();
    for _ in 0..4 {
        if let Some(pid) = provider_id {
            for key in [&norm, &"*".to_string()] {
                if let Ok(row) = conn.query_row(
                    "SELECT in_pm, out_pm, cache_read_pm, cache_create_pm
                     FROM provider_pricing WHERE provider_id = ?1 AND model_norm = ?2",
                    rusqlite::params![pid, key],
                    |r| {
                        Ok(RateSet {
                            in_pm: r.get(0)?,
                            out_pm: r.get(1)?,
                            cache_read_pm: r.get(2)?,
                            cache_create_pm: r.get(3)?,
                        })
                    },
                ) {
                    if !row.is_zero() {
                        return row;
                    }
                }
            }
        }
        match strip_date_tail(&norm) {
            Some(p) => norm = p,
            None => break,
        }
    }
    global_rates(conn, &model_norm.trim().to_lowercase())
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionFee {
    pub provider_id: i64,
    pub provider_name: String,
    pub model_norm: String,
    pub sub_fee_usd: f64,
    pub sub_included_tokens: i64,
    pub used_this_month: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionFees {
    pub fees: Vec<SubscriptionFee>,
    pub total_fee: f64,
}

/// 當月訂閱月費一覽（超額部分已在請求級費用中按基礎價計入，此處只匯總月費）。
pub(crate) fn subscription_fees(conn: &Connection) -> rusqlite::Result<SubscriptionFees> {
    let now = crate::fsutil::now_ms();
    let mut stmt = conn.prepare(
        "SELECT pp.provider_id, COALESCE(p.name, '(未知來源)'), pp.model_norm,
         pp.sub_fee_usd, pp.sub_included_tokens
         FROM provider_pricing pp LEFT JOIN providers p ON p.id = pp.provider_id
         WHERE pp.mode = 'subscription' ORDER BY pp.provider_id, pp.model_norm",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, f64>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;
    let mut fees = Vec::new();
    let mut total_fee = 0.0;
    for r in rows {
        let (pid, pname, model, fee, included) = r?;
        let used = month_usage(conn, pid, &model, now).unwrap_or(0);
        total_fee += fee;
        fees.push(SubscriptionFee {
            provider_id: pid,
            provider_name: pname,
            model_norm: model,
            sub_fee_usd: fee,
            sub_included_tokens: included,
            used_this_month: used,
        });
    }
    Ok(SubscriptionFees { fees, total_fee })
}
