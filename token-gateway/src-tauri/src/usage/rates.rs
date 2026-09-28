//! 費率決議：渠道精確 → 渠道 `*` → 全域 → 0（含日期尾綴回退與時段命中）。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;

use super::month_usage;
use super::provider_pricing::RateSet;

/// 本地時間是否落在 [start, end) 窗口內（允許跨夜，如 22:00-06:00）。
fn is_window_match(start: &str, end: &str, ts_ms: i64) -> bool {
    use chrono::TimeZone;
    let (Ok(ps), Ok(pe)) = (
        chrono::NaiveTime::parse_from_str(start, "%H:%M"),
        chrono::NaiveTime::parse_from_str(end, "%H:%M"),
    ) else {
        return false;
    };
    let now = chrono::Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(chrono::Local::now)
        .time();
    if ps <= pe {
        now >= ps && now < pe
    } else {
        // 跨夜窗口，如 22:00-06:00
        now >= ps || now < pe
    }
}

pub(crate) fn global_rates(conn: &Connection, model_norm: &str) -> RateSet {
    let (i, o, cr, cw): (f64, f64, f64, f64) = conn
        .query_row(
            "SELECT in_pm, out_pm, cache_read_pm, cache_create_pm FROM pricing WHERE model_norm = ?1",
            [model_norm],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap_or((0.0, 0.0, 0.0, 0.0));
    RateSet {
        in_pm: i,
        out_pm: o,
        cache_read_pm: cr,
        cache_create_pm: cw,
    }
}

/// 命中指定 (provider, model) 的第一個時段費率。
/// include_wild 為 true 時同時考慮 `*` 時段（自有優先）；false 僅看自有。
/// 未命中任何時段返回 None（调用方回退基礎費率或繼續下一輪）。
fn period_rates(
    conn: &Connection,
    provider_id: i64,
    model_norm: &str,
    ts_ms: i64,
    include_wild: bool,
) -> Option<RateSet> {
    let sql = if include_wild {
        "SELECT model_norm, start, end, in_pm, out_pm, cache_read_pm, cache_create_pm
         FROM pricing_periods WHERE provider_id = ?1 AND model_norm IN (?2, '*')
         ORDER BY CASE WHEN model_norm = ?2 THEN 0 ELSE 1 END, ord ASC, id ASC"
    } else {
        "SELECT model_norm, start, end, in_pm, out_pm, cache_read_pm, cache_create_pm
         FROM pricing_periods WHERE provider_id = ?1 AND model_norm = ?2
         ORDER BY ord ASC, id ASC"
    };
    let mut stmt = conn.prepare(sql).ok()?;
    let rows: Vec<(String, String, f64, f64, f64, f64)> = stmt
        .query_map(rusqlite::params![provider_id, model_norm], |r| {
            Ok((
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    for (start, end, ip, op, cr, cw) in rows {
        if is_window_match(&start, &end, ts_ms) {
            return Some(RateSet {
                in_pm: ip,
                out_pm: op,
                cache_read_pm: cr,
                cache_create_pm: cw,
            });
        }
    }
    None
}
/// 費率解析（優先級：渠道精確 → 渠道 `*` → 全域 → 0）。
///
/// usage 是基礎四率；tou 命中時段用時段價（自有時段優先於 `*`，按 ord），未命中用基礎；
/// subscription 期內免費額度用完前 0 費率。
///
/// 日期尾綴回退：`deepseek-v4-pro-0813` 自身查無定價時，逐級剝 `-MMDD` 試父級
/// （`kimi-k2-0905` 這種父級同樣無價的不受影響；最多回退 3 級）。
pub(crate) fn resolve_rates(
    conn: &Connection,
    provider_id: Option<i64>,
    model_norm: &str,
    ts_ms: i64,
) -> RateSet {
    let mut norm = model_norm.trim().to_lowercase();
    for _ in 0..3 {
        let r = resolve_rates_inner(conn, provider_id, &norm, ts_ms);
        if !r.is_zero() {
            return r;
        }
        match strip_date_tail(&norm) {
            Some(p) => norm = p,
            None => return r,
        }
    }
    resolve_rates_inner(conn, provider_id, &norm, ts_ms)
}

/// `-MMDD` 尾綴（恰好 4 位數字）剝一層；否則 None。
pub(crate) fn strip_date_tail(norm: &str) -> Option<String> {
    let (head, tail) = norm.rsplit_once('-')?;
    if !head.is_empty() && tail.len() == 4 && tail.bytes().all(|b| b.is_ascii_digit()) {
        Some(head.to_string())
    } else {
        None
    }
}

fn resolve_rates_inner(
    conn: &Connection,
    provider_id: Option<i64>,
    model_norm: &str,
    ts_ms: i64,
) -> RateSet {
    let norm = model_norm.trim().to_lowercase();
    if let Some(pid) = provider_id {
        let wild = "*".to_string();
        for key in [&norm, &wild] {
            let row: rusqlite::Result<(String, f64, f64, f64, f64, i64)> =
                conn.query_row(
                    "SELECT mode, in_pm, out_pm, cache_read_pm, cache_create_pm, sub_included_tokens
                 FROM provider_pricing WHERE provider_id = ?1 AND model_norm = ?2",
                    rusqlite::params![pid, key],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    },
                );
            if let Ok((mode, ip, op, cr, cw, included)) = row {
                let base = RateSet {
                    in_pm: ip,
                    out_pm: op,
                    cache_read_pm: cr,
                    cache_create_pm: cw,
                };
                match mode.as_str() {
                    // 訂閱優先：額度內一律免費，時段不介入。
                    "subscription" => {
                        // 額度內免費；-1 表示無限
                        if included >= 0 {
                            let used =
                                month_usage(conn, pid, key.as_str(), ts_ms).unwrap_or(0);
                            if used < included {
                                return RateSet::zero();
                            }
                        } else {
                            return RateSet::zero();
                        }
                        return base;
                    }
                    // usage / tou：先看時段（自有優先於 `*`），未命中回基礎價。
                    _ => {
                        if let Some(rates) =
                            period_rates(conn, pid, key.as_str(), ts_ms, true)
                        {
                            return rates;
                        }
                        return base;
                    }
                }
            }
            // 無定價行時，獨立時段仍可生效（僅自有模型；`*` 留給下一輪）。
            if let Some(rates) = period_rates(conn, pid, key.as_str(), ts_ms, false) {
                return rates;
            }
        }
    }
    global_rates(conn, &norm)
}

pub(crate) fn rate_cost(rates: &RateSet, in_tok: i64, out_tok: i64, cr: i64, cw: i64) -> f64 {
    (in_tok as f64 * rates.in_pm
        + out_tok as f64 * rates.out_pm
        + cr as f64 * rates.cache_read_pm
        + cw as f64 * rates.cache_create_pm)
        / 1_000_000.0
}
