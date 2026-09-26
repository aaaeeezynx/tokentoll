//! M4：用量聚合查詢 + 定價管理。
//!
//! 時間戳統一用毫秒（`request_logs.ts`）。過濾條件全部可選，前端按
//! 今日 / 7 天 / 30 天換算 `start_ts` 後傳入。

use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

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
        cache_hit_rate: if itok + cr > 0 {
            cr as f64 / (itok + cr) as f64
        } else {
            0.0
        },
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
        "SELECT (ts/?1)*?1 AS b, COALESCE(app,'unknown'), COUNT(*),
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w} GROUP BY b, COALESCE(app,'unknown') ORDER BY b ASC"
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

fn csv_esc(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 按過濾條件匯出請求明細 CSV（UTF-8，時間為本地時區）。
pub(crate) fn export_csv(conn: &Connection, f: &UsageFilter) -> rusqlite::Result<String> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT ts, COALESCE(app,'unknown'), COALESCE(model_raw,''), in_tok, out_tok,
         cache_read, cache_write, (in_tok+out_tok+cache_read+cache_write),
         cost_usd, status, COALESCE(source,'')
         FROM request_logs WHERE {w} ORDER BY ts ASC"
    ))?;
    let mut out = String::from(
        "time,app,model,in_tok,out_tok,cache_read,cache_write,total_tokens,cost_usd,status,source\n",
    );
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, f64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, String>(10)?,
        ))
    })?;
    for r in rows {
        let (ts, app, model, itok, otok, cr, cw, total, cost, status, source) = r?;
        let time = chrono::DateTime::from_timestamp_millis(ts)
            .map(|u| {
                u.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{:.4},{},{}\n",
            time,
            csv_esc(&app),
            csv_esc(&model),
            itok,
            otok,
            cr,
            cw,
            total,
            cost,
            status,
            csv_esc(&source)
        ));
    }
    Ok(out)
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

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub provider_id: Option<i64>,
    pub provider_name: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost_usd: f64,
    pub success_rate: f64,
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
         COALESCE(SUM(l.in_tok+l.out_tok+l.cache_read+l.cache_write),0),
         COALESCE(SUM(l.cost_usd),0),
         COALESCE(SUM(CASE WHEN l.status BETWEEN 200 AND 299 THEN 1 ELSE 0 END),0)
         FROM request_logs l LEFT JOIN providers p ON p.id = l.provider_id
         WHERE {w} GROUP BY l.provider_id ORDER BY 4 DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let requests: i64 = r.get(2)?;
        let ok: i64 = r.get(5)?;
        Ok(ProviderStat {
            provider_id: r.get(0)?,
            provider_name: r.get(1)?,
            requests,
            tokens: r.get(3)?,
            cost_usd: r.get(4)?,
            success_rate: if requests > 0 {
                ok as f64 / requests as f64
            } else {
                0.0
            },
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
    pub tokens: i64,
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
         COALESCE(SUM(in_tok+out_tok+cache_read+cache_write),0), COALESCE(SUM(cost_usd),0)
         FROM request_logs WHERE {w} GROUP BY 1 ORDER BY 5 DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok(ModelStat {
            model: r.get(0)?,
            requests: r.get(1)?,
            in_tok: r.get(2)?,
            out_tok: r.get(3)?,
            tokens: r.get(4)?,
            cost_usd: r.get(5)?,
        })
    })?;
    rows.collect()
}

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
    fn zero() -> Self {
        Self {
            in_pm: 0.0,
            out_pm: 0.0,
            cache_read_pm: 0.0,
            cache_create_pm: 0.0,
        }
    }
    fn is_zero(&self) -> bool {
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

fn global_rates(conn: &Connection, model_norm: &str) -> RateSet {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn summary_and_filters() {
        let (_d, conn) = seed_db();
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        add_log(&conn, 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 2000, "codex", Some(1), "m1", 20, 10, 0.2, 500);
        add_log(&conn, 3000, "claude", Some(2), "m2", 30, 0, 0.3, 200);
        let s = summary(&conn, &f0).unwrap();
        assert_eq!((s.requests, s.ok_requests), (3, 2));
        assert_eq!((s.in_tok, s.out_tok), (60, 15));
        assert!((s.cost_usd - 0.6).abs() < 1e-9);
        assert!((s.success_rate - 2.0 / 3.0).abs() < 1e-9);
        // 時間過濾
        let s2 = summary(
            &conn,
            &UsageFilter {
                start_ts: 2000,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s2.requests, 2);
        // 應用過濾
        let s3 = summary(
            &conn,
            &UsageFilter {
                app: Some("claude".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((s3.requests, s3.in_tok), (1, 30));
    }

    #[test]
    fn trend_buckets_and_group_bys() {
        let (_d, conn) = seed_db();
        // ms 時間戳：day1=86_400_000, day2=172_800_000
        add_log(&conn, 86_400_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 172_800_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let t = trend(&conn, &f0, 86400).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].ts, 86_400_000);
        assert_eq!(t[1].tokens, 15);
        let bp = by_provider(&conn, &f0).unwrap();
        assert_eq!(bp.len(), 1);
        assert_eq!(bp[0].requests, 2);
        let bm = by_model(&conn, &f0).unwrap();
        assert_eq!(bm.len(), 1);
        assert_eq!(bm[0].model, "m1");
    }

    #[test]
    fn export_csv_respects_filter() {
        let (_d, conn) = seed_db();
        add_log(&conn, 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 2000, "claude", Some(1), "m,2", 20, 0, 0.2, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let csv = export_csv(&conn, &f0).unwrap();
        assert!(csv.starts_with("time,app,model,"));
        assert!(csv.contains("codex"));
        // 含逗號的模型名被引號包裹
        assert!(csv.contains("\"m,2\""));
        let csv2 = export_csv(
            &conn,
            &UsageFilter {
                app: Some("claude".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!csv2.contains("codex"));
    }

    #[test]
    fn price_quote_math_and_source() {
        let (_d, conn) = seed_db();
        let (pid, pname): (i64, String) = conn
            .query_row("SELECT id, name FROM providers LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        pp_upsert(
            &conn,
            &ProviderPricingInput {
                provider_id: pid,
                model_norm: "m1".into(),
                mode: "usage".into(),
                in_pm: 2.0,
                out_pm: 8.0,
                cache_read_pm: 0.2,
                cache_create_pm: 0.0,
                sub_fee_usd: 0.0,
                sub_included_tokens: -1,
            },
        )
        .unwrap();
        let q = price_quote(&conn, Some(pid), "M1", 1_000_000, 500_000, 0, 0).unwrap();
        assert!((q.cost_usd - 6.0).abs() < 1e-9);
        assert_eq!(q.source, format!("{pname}・模型"));
        assert_eq!(q.mode, "usage");
        // 訂閱行：回傳月費/額度/已用
        pp_upsert(
            &conn,
            &ProviderPricingInput {
                provider_id: pid,
                model_norm: "msub".into(),
                mode: "subscription".into(),
                in_pm: 2.0,
                out_pm: 8.0,
                cache_read_pm: 0.0,
                cache_create_pm: 0.0,
                sub_fee_usd: 20.0,
                sub_included_tokens: 1000,
            },
        )
        .unwrap();
        let qs = price_quote(&conn, Some(pid), "msub", 100, 0, 0, 0).unwrap();
        assert_eq!(qs.mode, "subscription");
        assert!((qs.sub_fee_usd - 20.0).abs() < 1e-9);
        assert_eq!(qs.sub_included_tokens, 1000);
        assert_eq!(qs.cost_usd, 20.0);
        assert_eq!(qs.sub_over_tokens, 0);
        // 本月已用 900，場景 1000 → 超 900，按基礎輸入價 $2/M
        add_log(
            &conn,
            crate::fsutil::now_ms(),
            "codex",
            Some(pid),
            "msub",
            900,
            0,
            0.0,
            200,
        );
        let qo = price_quote(&conn, Some(pid), "msub", 1000, 0, 0, 0).unwrap();
        assert_eq!(qo.sub_used_this_month, 900);
        assert_eq!(qo.sub_over_tokens, 900);
        assert!((qo.cost_usd - (20.0 + 900.0 / 1e6 * 2.0)).abs() < 1e-9);
        let fees = subscription_fees(&conn).unwrap();
        assert!(fees.fees.iter().any(|f| f.provider_id == pid && f.model_norm == "msub"));
        assert!(fees.total_fee >= 20.0);
        // 全域回退
        let q2 = price_quote(&conn, Some(pid), "nope", 1_000_000, 0, 0, 0).unwrap();
        assert_eq!(q2.source, "未定價（記 0）");
        assert_eq!(q2.cost_usd, 0.0);
    }

    #[test]
    fn resolve_filter_validates_custom() {
        use crate::commands::resolve_filter;
        assert!(resolve_filter("custom", None, None, None, None, None).is_err());
        assert!(resolve_filter("custom", None, None, None, Some(2000), Some(1000)).is_err());
        let now = crate::fsutil::now_ms();
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 400 * 86400 * 1000), Some(now))
                .is_err()
        );
        assert!(
            resolve_filter("custom", None, None, None, Some(now + 3600 * 1000), Some(now + 7200 * 1000))
                .is_err()
        );
        let f =
            resolve_filter("custom", None, None, None, Some(now - 1000), Some(now)).unwrap();
        assert_eq!(f.end_ts, Some(now));
    }

    #[test]
    fn trend_by_app_splits_apps_per_bucket() {
        let (_d, conn) = seed_db();
        add_log(&conn, 86_400_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 86_400_000 + 2000, "claude", Some(1), "m1", 20, 0, 0.2, 200);
        add_log(&conn, 172_800_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let t = trend_by_app(&conn, &f0, 86400).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].ts, 86_400_000);
        assert_eq!(t[0].app, "claude");
        assert_eq!(t[0].tokens, 20);
        assert_eq!(t[1].app, "codex");
        assert_eq!(t[1].tokens, 15);
        assert_eq!(t[2].ts, 172_800_000);
    }

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
    fn pricing_crud() {
        let (_d, conn) = seed_db();
        pricing_upsert(
            &conn,
            &PricingInput {
                model_norm: "My-Model".into(),
                in_pm: 1.0,
                out_pm: 2.0,
                cache_read_pm: 0.1,
                cache_create_pm: 0.2,
            },
        )
        .unwrap();
        let all = pricing_list(&conn).unwrap();
        assert!(all.iter().any(|p| p.model_norm == "my-model" && p.source == "custom"));
        assert!(pricing_upsert(
            &conn,
            &PricingInput {
                model_norm: "bad".into(),
                in_pm: -1.0,
                out_pm: 0.0,
                cache_read_pm: 0.0,
                cache_create_pm: 0.0,
            },
        )
        .is_err());
        pricing_delete(&conn, "my-model").unwrap();
        assert!(pricing_delete(&conn, "my-model").is_err());
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

    #[test]
    fn resolve_priority_and_modes() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "flatkey");
        // 全域回退：無渠道定價時用全域 seed
        let r = resolve_rates(&conn, Some(pid), "deepseek-v4-flash", ts_at(10, 0));
        assert!((r.in_pm - 0.14).abs() < 1e-9);
        // 渠道 * 通配優先於全域
        pp_upsert(&conn, &pp(pid, "*", "usage")).unwrap();
        let r = resolve_rates(&conn, Some(pid), "whatever-model", ts_at(10, 0));
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // 精確優先於通配
        let mut exact = pp(pid, "m1", "usage");
        exact.in_pm = 5.0;
        pp_upsert(&conn, &exact).unwrap();
        let r = resolve_rates(&conn, Some(pid), "m1", ts_at(10, 0));
        assert!((r.in_pm - 5.0).abs() < 1e-9);
        // 未知模型無定價 → 0
        let r = resolve_rates(&conn, None, "nope", ts_at(10, 0));
        assert_eq!(r, RateSet::zero());
    }

    #[test]
    fn resolve_date_tail_fallback() {
        let (_d, conn) = seed_db();
        // deepseek-v4-pro 有種子價；-0813 尾綴回退命中父級
        let r = resolve_rates(&conn, None, "deepseek-v4-pro-0813", 0);
        assert!((r.in_pm - 1.74).abs() < 1e-9);
        assert!((r.out_pm - 3.48).abs() < 1e-9);
        // 渠道鏈路同樣回退（無渠道行 → 全域父級）
        let pid = provider_id(&conn, "flatkey");
        let r = resolve_rates(&conn, Some(pid), "deepseek-v4-pro-0813", 0);
        assert!((r.in_pm - 1.74).abs() < 1e-9);
        // kimi-k2-0905 父級無價 → 保持 0（行為不變）
        assert_eq!(
            resolve_rates(&conn, None, "kimi-k2-0905", 0),
            RateSet::zero()
        );
        // 非日期尾綴不受影響
        assert!(resolve_rates(&conn, None, "gpt-5", 0).in_pm > 0.0);
    }

    #[test]
    fn resolve_tou_multi_periods() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "zen");
        pp_upsert(&conn, &pp(pid, "m-tou", "tou")).unwrap();
        // deepseek 式多時段：早峰 08:00-12:00、晚峰 18:00-22:00、深夜跨夜 22:00-06:00
        pp_period_add(&conn, &period(pid, "m-tou", "早峰", "08:00", "12:00")).unwrap();
        let mut evening = period(pid, "m-tou", "晚峰", "18:00", "22:00");
        evening.in_pm = 3.0;
        pp_period_add(&conn, &evening).unwrap();
        let night = period(pid, "m-tou", "深夜", "22:00", "06:00");
        pp_period_add(&conn, &night).unwrap();
        // 各時段命中各自費率
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(10, 0));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(19, 0));
        assert!((r.in_pm - 3.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(23, 30));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(5, 59));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        // 空隙（12:00-18:00）回退基礎價
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(14, 0));
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // `*` 時段作為其他模型的回退（自有時段優先）
        let mut promo = period(pid, "*", "全天促銷", "00:00", "23:59");
        promo.in_pm = 9.0;
        pp_period_add(&conn, &promo).unwrap();
        let r = resolve_rates(&conn, Some(pid), "other-model", ts_at(3, 0));
        assert!((r.in_pm - 9.0).abs() < 1e-9);
        // 自有時段優先於 *（同為 03:00，m-tou 有深夜時段 2.0）
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(3, 0));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        // 時段 CRUD 與校驗
        let list = pp_periods(&conn, pid, "m-tou").unwrap();
        assert_eq!(list.len(), 3);
        let id = list[0].id;
        let mut upd = period(pid, "m-tou", "早峰改", "08:00", "12:00");
        upd.ord = 5;
        pp_period_update(&conn, id, &upd).unwrap();
        assert!(pp_period_update(&conn, 999999, &upd).is_err());
        let mut bad = period(pid, "m-tou", "壞", "25:00", "12:00");
        bad.start = "25:00".into();
        assert!(pp_period_add(&conn, &bad).is_err());
        pp_period_delete(&conn, id).unwrap();
        assert!(pp_period_delete(&conn, id).is_err());
        assert_eq!(pp_periods(&conn, pid, "m-tou").unwrap().len(), 2);
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

    #[test]
    fn resolve_subscription_included_then_overage() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "nvidia");
        pp_upsert(&conn, &pp(pid, "m-sub", "subscription")).unwrap();
        let now = crate::fsutil::now_ms();
        // 期內 400 < 1000 → 免費
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,'codex',?2,'m-sub','m-sub',300,100,0,10,200,0)",
            rusqlite::params![now, pid],
        )
        .unwrap();
        let r = resolve_rates(&conn, Some(pid), "m-sub", now);
        assert_eq!(r, RateSet::zero());
        // 再用 700 → 累計 1100 > 1000 → 超額按基礎價
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,'codex',?2,'m-sub','m-sub',500,200,0,10,200,0)",
            rusqlite::params![now, pid],
        )
        .unwrap();
        let r = resolve_rates(&conn, Some(pid), "m-sub", now);
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // 通配訂閱按渠道匯總
        pp_upsert(&conn, &pp(pid, "*", "subscription")).unwrap();
        let list = pp_list(&conn, Some(pid), now).unwrap();
        let wild = list.iter().find(|p| p.model_norm == "*").unwrap();
        assert_eq!(wild.used_this_month, 1100);
        // 非法輸入被拒
        let mut bad = pp(pid, "x", "nope");
        bad.mode = "nope".into();
        assert!(pp_upsert(&conn, &bad).is_err());
        let mut bad2 = pp(pid, "", "usage");
        bad2.model_norm = "".into();
        assert!(pp_upsert(&conn, &bad2).is_err());
        pp_delete(&conn, pid, "m-sub").unwrap();
        assert!(pp_delete(&conn, pid, "m-sub").is_err());
    }
}
