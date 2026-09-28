//! 用量查詢與報價命令，含 range 解析的共用純函式（原 `commands.rs` 的「用量」段）。

use tauri::State;

use crate::db::DbState;
use crate::usage;

// ---------------------------------------------------------------- 用量 ---

/// 範圍換算為起始毫秒時間戳（本地時區；today 取當地 0 點）。
pub(crate) fn range_start(range: &str) -> i64 {
    let now = chrono::Local::now();
    match range {
        "today" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|d| d.and_local_timezone(chrono::Local).single())
            .map(|d| d.timestamp_millis())
            .unwrap_or(0),
        "7d" => now.timestamp_millis() - 7 * 86400 * 1000,
        "30d" => now.timestamp_millis() - 30 * 86400 * 1000,
        "90d" => now.timestamp_millis() - 90 * 86400 * 1000,
        "180d" => now.timestamp_millis() - 180 * 86400 * 1000,
        "365d" => now.timestamp_millis() - 365 * 86400 * 1000,
        _ => 0,
    }
}

/// 自訂期間（range="custom"）必須帶起止；起必須早於止，跨度上限 365 天，止不能是未來。
pub(crate) fn resolve_filter(    range: &str,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<usage::UsageFilter, String> {
    let now = chrono::Local::now().timestamp_millis();
    let start = start_ts.unwrap_or_else(|| range_start(range));
    let end = end_ts.unwrap_or(now);
    if range == "custom" && (start_ts.is_none() || end_ts.is_none()) {
        return Err("自訂期間需指定起止時間".to_string());
    }
    if start >= end {
        return Err("起始時間必須早於結束時間".to_string());
    }
    if end - start > 365 * 86400 * 1000 {
        return Err("自訂期間跨度上限 365 天".to_string());
    }
    if end > now + 60 * 1000 {
        return Err("結束時間不能是未來".to_string());
    }
    Ok(usage::UsageFilter {
        start_ts: start,
        end_ts: if end_ts.is_some() { Some(end) } else { None },
        app: app.filter(|s| !s.is_empty()),
        provider_id,
        model: model.filter(|s| !s.is_empty()),
    })
}

/// bucket 粒度按實際跨度切（自訂長跨度按週）。
pub(crate) fn range_bucket_span(start_ts: i64, end_ts: Option<i64>) -> i64 {
    let now = chrono::Local::now().timestamp_millis();
    let span = end_ts.unwrap_or(now) - start_ts;
    if span <= 2 * 86400 * 1000 {
        3600
    } else if span <= 186 * 86400 * 1000 {
        86400
    } else {
        7 * 86400
    }
}

#[tauri::command]
pub fn usage_summary(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<usage::UsageSummary, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::summary(&conn, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_trend(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::TrendBucket>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    let bucket = range_bucket_span(f.start_ts, f.end_ts);
    usage::trend(&conn, &f, bucket).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_trend_by_app(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::TrendAppRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    let bucket = range_bucket_span(f.start_ts, f.end_ts);
    usage::trend_by_app(&conn, &f, bucket).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_export_csv(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::export_csv(&conn, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_by_provider(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::ProviderStat>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::by_provider(&conn, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_by_model(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::ModelStat>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::by_model(&conn, &f).map_err(|e| e.to_string())
}

/// 分本機工具統計（「工具」視角）。唯一能並排比較各工具的來源。
#[tauri::command]
pub fn usage_by_app(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::AppStat>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::by_app(&conn, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn usage_hourly(
    db: State<DbState>,
    range: String,
    app: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<Vec<usage::HourlyBucket>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let f = resolve_filter(&range, app, provider_id, model, start_ts, end_ts)?;
    usage::hourly(&conn, &f).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn subscription_fees(db: State<DbState>) -> Result<usage::SubscriptionFees, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::subscription_fees(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn price_quote(
    db: State<DbState>,
    provider_id: Option<i64>,
    model_norm: String,
    in_tok: i64,
    out_tok: i64,
    cache_read: i64,
    cache_write: i64,
) -> Result<usage::PriceQuote, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::price_quote(
        &conn,
        provider_id,
        &model_norm,
        in_tok,
        out_tok,
        cache_read,
        cache_write,
    )
    .map_err(|e| e.to_string())
}
