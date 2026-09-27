//! Tauri 命令層（M1）：只讀狀態查詢。M2 起陸續接入增刪改查。

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::{self, DbState, SCHEMA_VERSION};
use crate::history;
use crate::keys;
use crate::proxy::{self, ProxyState};
use crate::tools;
use crate::trace;
use crate::usage;

#[derive(Serialize)]
pub struct DbStatus {
    pub path: String,
    pub schema_version: i32,
    pub provider_count: i64,
    pub key_count: i64,
    pub log_count: i64,
}

#[tauri::command]
pub fn db_status(db: State<DbState>) -> Result<DbStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    Ok(DbStatus {
        path: db.path.to_string_lossy().to_string(),
        schema_version: SCHEMA_VERSION,
        provider_count: count("SELECT COUNT(*) FROM providers"),
        key_count: count("SELECT COUNT(*) FROM local_keys"),
        log_count: count("SELECT COUNT(*) FROM request_logs"),
    })
}

#[derive(Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub listen: String,
    pub port: u16,
    pub started_at: Option<i64>,
}

/// 真實狀態：讀 ProxyState（M1 存根已替換）。
#[tauri::command]
pub fn proxy_status(proxy: State<ProxyState>) -> ProxyStatus {
    let inner = proxy.inner.lock().unwrap_or_else(|e| e.into_inner());
    match inner.as_ref() {
        Some(r) => ProxyStatus {
            running: true,
            listen: "127.0.0.1".to_string(),
            port: r.port,
            started_at: Some(r.started_at),
        },
        None => ProxyStatus {
            running: false,
            listen: "127.0.0.1".to_string(),
            port: 0,
            started_at: None,
        },
    }
}

/// C 方案：僅檢測連接埠是否可用，不啟動。
#[tauri::command]
pub fn proxy_check_port(port: u16) -> Result<(), String> {
    proxy::check_port(port)
}

#[tauri::command]
pub async fn proxy_start(
    db: State<'_, DbState>,
    proxy: State<'_, ProxyState>,
    port: u16,
) -> Result<ProxyStatus, String> {
    if !(1..=65535).contains(&port) {
        return Err("連接埠範圍應為 1–65535".to_string());
    }
    {
        let inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        if let Some(r) = inner.as_ref() {
            if r.port == port {
                return Ok(ProxyStatus {
                    running: true,
                    listen: "127.0.0.1".to_string(),
                    port,
                    started_at: Some(r.started_at),
                });
            }
            return Err(format!(
                "網關已在連接埠 {} 執行，請先停止再切換連接埠",
                r.port
            ));
        }
    }
    proxy::check_port(port)?;
    let db_path = db.path.clone();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                format!("連接埠 {port} 已被佔用（本機可能有 cc-switch 或其他代理在執行）")
            } else {
                format!("連接埠 {port} 綁定失敗：{e}")
            }
        })?;
    let handle = tokio::spawn(async move {
        if let Err(e) = proxy::serve(db_path, listener).await {
            eprintln!("gateway error: {e}");
        }
    });
    {
        let mut inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        *inner = Some(proxy::RunningProxy {
            port,
            started_at: crate::fsutil::now_ms(),
            handle,
        });
    }
    Ok(ProxyStatus {
        running: true,
        listen: "127.0.0.1".to_string(),
        port,
        started_at: None,
    })
}

#[tauri::command]
pub fn proxy_stop(proxy: State<ProxyState>) -> Result<(), String> {
    let mut inner = proxy.inner.lock().map_err(|e| e.to_string())?;
    if let Some(r) = inner.take() {
        r.handle.abort();
    }
    Ok(())
}

// ---------------------------------------------------------------- 設定 ---

#[derive(Serialize)]
pub struct AppSettings {
    pub gateway_port: u16,
    pub auto_start_proxy: bool,
    pub accent: String,
}

const ACCENT_IDS: &[&str] = &["blue", "green", "purple", "pink"];

#[tauri::command]
pub fn settings_get(db: State<DbState>) -> Result<AppSettings, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let port = db::get_setting(&conn, "gateway_port")
        .and_then(|v| v.parse().ok())
        .unwrap_or(db::DEFAULT_GATEWAY_PORT);
    let auto = db::get_setting(&conn, "auto_start_proxy")
        .is_some_and(|v| v == "1");
    let accent = db::get_setting(&conn, "accent")
        .filter(|v| ACCENT_IDS.contains(&v.as_str()))
        .unwrap_or_else(|| "blue".to_string());
    Ok(AppSettings {
        gateway_port: port,
        auto_start_proxy: auto,
        accent,
    })
}

/// 外觀強調色持久化（白名單校驗；SQLite 提交即落盤，不受 WebView 儲存刷盤時機影響）
#[tauri::command]
pub fn settings_set_accent(db: State<DbState>, accent: String) -> Result<String, String> {
    if !ACCENT_IDS.contains(&accent.as_str()) {
        return Err(format!("未知的強調色：{accent}"));
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "accent", &accent).map_err(|e| e.to_string())?;
    Ok(accent)
}

#[tauri::command]
pub fn settings_set(
    db: State<DbState>,
    proxy: State<ProxyState>,
    gateway_port: u16,
    auto_start_proxy: bool,
) -> Result<AppSettings, String> {
    if !(1..=65535).contains(&gateway_port) {
        return Err("連接埠範圍應為 1–65535".to_string());
    }
    {
        let inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        if let Some(r) = inner.as_ref() {
            if r.port != gateway_port {
                return Err(format!(
                    "網關正在連接埠 {} 執行，請先停止再修改連接埠",
                    r.port
                ));
            }
        }
    }
    proxy::check_port(gateway_port)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "gateway_port", &gateway_port.to_string())
        .map_err(|e| e.to_string())?;
    db::set_setting(
        &conn,
        "auto_start_proxy",
        if auto_start_proxy { "1" } else { "0" },
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    settings_get(db)
}

// ---------------------------------------------------------------- Key ---

#[tauri::command]
pub fn keys_list(db: State<DbState>) -> Result<Vec<keys::LocalKey>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::list_keys(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn key_create(
    db: State<DbState>,
    input: keys::KeyInput,
) -> Result<keys::KeyCreated, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::create_key(&conn, &input)
}

#[tauri::command]
pub fn key_update(
    db: State<DbState>,
    id: i64,
    input: keys::KeyInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::update_key(&conn, id, &input)
}

#[tauri::command]
pub fn key_set_enabled(
    db: State<DbState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::set_key_enabled(&conn, id, enabled)
}

#[tauri::command]
pub fn key_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::delete_key(&conn, id)
}

#[tauri::command]
pub fn key_rotate(db: State<DbState>, id: i64) -> Result<keys::KeyCreated, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::rotate_key(&conn, id)
}

#[tauri::command]
pub fn key_reveal(db: State<DbState>, id: i64) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    keys::reveal_key(&conn, id)
}

// ---------------------------------------------------------------- 用量 ---

/// 範圍換算為起始毫秒時間戳（本地時區；today 取當地 0 點）。
fn range_start(range: &str) -> i64 {
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
pub(crate) fn resolve_filter(
    range: &str,
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
    if end - start > 366 * 86400 * 1000 {
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
fn range_bucket_span(start_ts: i64, end_ts: Option<i64>) -> i64 {
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

#[tauri::command]
pub fn pricing_list(db: State<DbState>) -> Result<Vec<usage::Pricing>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pricing_upsert(
    db: State<DbState>,
    input: usage::PricingInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_upsert(&conn, &input)
}

#[tauri::command]
pub fn pricing_delete(db: State<DbState>, model_norm: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_delete(&conn, &model_norm)
}

/// 清空全域預設定價（種子有版本旗標，不會在重啟後復活）。
#[tauri::command]
pub fn pricing_clear(db: State<DbState>) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM pricing", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn pp_list(
    db: State<DbState>,
    provider_id: Option<i64>,
) -> Result<Vec<usage::ProviderPricing>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let now = crate::fsutil::now_ms();
    usage::pp_list(&conn, provider_id, now).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pp_upsert(
    db: State<DbState>,
    input: usage::ProviderPricingInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_upsert(&conn, &input)
}

#[tauri::command]
pub fn pp_delete(
    db: State<DbState>,
    provider_id: i64,
    model_norm: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_delete(&conn, provider_id, &model_norm)
}

#[tauri::command]
pub fn pp_periods(
    db: State<DbState>,
    provider_id: i64,
    model_norm: String,
) -> Result<Vec<usage::PricingPeriod>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_periods(&conn, provider_id, &model_norm).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pp_period_add(
    db: State<DbState>,
    input: usage::PricingPeriodInput,
) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_add(&conn, &input)
}

#[tauri::command]
pub fn pp_period_update(
    db: State<DbState>,
    id: i64,
    input: usage::PricingPeriodInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_update(&conn, id, &input)
}

#[tauri::command]
pub fn pp_period_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_delete(&conn, id)
}

// ---------------------------------------------------------------- 日誌 ---

#[tauri::command]
pub fn recent_logs(
    db: State<DbState>,
    limit: i64,
) -> Result<Vec<proxy::LogRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    proxy::recent_logs(&conn, limit).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 歷史回填 ---

/// 掃描本機會話來源（乾跑，只統計文件數/位元組/會話數，不寫庫）。
#[tauri::command]
pub fn history_scan() -> Result<Vec<history::ScanTool>, String> {
    let home = dirs::home_dir().ok_or("找不到用戶主目錄".to_string())?;
    Ok(history::scan_history(&home))
}

/// 執行回填（87MB 級解析放 blocking 執行緒，避免卡住命令執行緒）。
#[tauri::command]
pub async fn history_import(
    app: AppHandle,
    tools: Vec<String>,
) -> Result<history::ImportSummary, String> {
    for t in &tools {
        if !["claude", "codex", "opencode"].contains(&t.as_str()) {
            return Err(format!("未知工具：{t}"));
        }
    }
    tokio::task::spawn_blocking(move || {
        let st = app.state::<DbState>();
        let conn = st.conn.lock().map_err(|e| e.to_string())?;
        history::import_history(&conn, &tools)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn app_data_dir(app: AppHandle) -> Result<String, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub fn tools_detect(port: u16) -> Vec<tools::ToolStatus> {
    tools::detect_tools(port)
}

#[tauri::command]
pub async fn tool_versions() -> Vec<tools::ToolVersion> {
    tauri::async_runtime::spawn_blocking(tools::tool_versions)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn tool_latest() -> Vec<tools::ToolLatest> {
    tauri::async_runtime::spawn_blocking(tools::tool_latest)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn tool_update(app: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || tools::tool_update_run(&app))
        .await
        .map_err(|e| e.to_string())?
}

fn normalize_switch_request(
    db: &DbState,
    req: &mut tools::SwitchRequest,
    port: u16,
) -> Result<(), String> {
    // base_url 一律指向本網關（唯一模式；`via_gateway` 已於 2026-09-28 移除）。
    req.base_url = tools::gateway_url(port, &req.app);
    if let Some(pid) = req.provider_id {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let api_format: Option<String> = conn
            .query_row(
                "SELECT api_format FROM providers WHERE id=?1 AND enabled=1",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .ok();
        drop(conn);
        let Some(api_format) = api_format else {
            return Err("該來源不存在或已停用（可能已被刪除），請重新選擇來源接管".into());
        };
        req.provider_format = Some(api_format);
    }
    Ok(())
}

#[tauri::command]
pub fn switch_plan(
    db: State<DbState>,
    mut req: tools::SwitchRequest,
    port: u16,
) -> Result<tools::SwitchPlan, String> {
    normalize_switch_request(&db, &mut req, port)?;
    tools::plan_switch(&req, port)
}

#[tauri::command]
pub fn switch_apply(
    app: AppHandle,
    db: State<DbState>,
    mut req: tools::SwitchRequest,
    port: u16,
) -> Result<tools::SwitchResult, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    normalize_switch_request(&db, &mut req, port)?;
    // 直連上游模式：Key 欄留空 → 以所選來源的上游 Key 直連（不經本地 Key）。
    // 此處從庫中取出上游 Key 注入請求；config 側明文寫入（計劃頁已警告）。
    // 對所有工具一致：claude 寫入自家配置，codex/opencode 寫 provider 段。
    if req.direct_upstream {
        let pid = req.provider_id.ok_or("直連模式需先選擇來源")?;
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let up: String = conn
            .query_row(
                "SELECT api_key FROM providers WHERE id=?1 AND enabled=1",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .map_err(|_| "直連模式需該來源已啟用".to_string())?;
        if up.trim().is_empty() {
            return Err("該來源未填寫上游 API Key，無法直連（請先在來源詳情填寫）".to_string());
        }
        req.api_key = up;
    }
    // Codex 模型目錄按需產生（模板收編自網關自持副本，不再依賴 cc-switch）
    let catalog = if req.gen_catalog && req.app == "codex" {
        let pid = req
            .provider_id
            .ok_or("產生模型目錄需先選擇渠道")?;
        let home = tools::user_home()?;
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        Some(
            crate::models::codex_catalog_generate(&conn, &home, &data, pid, req.catalog_union)
                .map(|p| p.to_string_lossy().to_string())?,
        )
    } else {
        None
    };
    tools::apply_switch(&data, req, port, catalog)
}

#[tauri::command]
pub fn switch_restore(
    app: AppHandle,
    app_name: String,
    port: u16,
) -> Result<String, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    tools::restore_backup_port(&data, &app_name, port)
}

#[tauri::command]
pub fn codex_doctor(port: u16) -> Vec<String> {
    tools::codex_doctor(port)
}

// ---------------------------------------------------------------- 模型 ---

#[tauri::command]
pub async fn catalog_fetch(
    app: AppHandle,
    provider_id: i64,
    api_key: Option<String>,
    base_url: Option<String>,
) -> Result<crate::models::CatalogCache, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        // 短鎖讀配置 → 放鎖做網路（最多 30s）→ 短鎖寫快取：全程不佔主執行緒、不扣全局鎖做網路。
        let cfg = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            crate::models::catalog_read_cfg(
                &conn,
                provider_id,
                api_key.as_deref(),
                base_url.as_deref(),
            )?
        };
        let ids = crate::models::catalog_fetch_http(&cfg)?;
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        crate::models::catalog_write(&conn, provider_id, ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn catalog_get(
    db: State<DbState>,
    provider_id: i64,
) -> Result<crate::models::CatalogCache, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::catalog_get(&conn, provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn models_list(
    db: State<DbState>,
    provider_id: i64,
) -> Result<Vec<crate::models::UsableModel>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::models_list(&conn, provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn model_counts(db: State<DbState>) -> Result<Vec<(i64, i64)>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_counts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn model_add(
    db: State<DbState>,
    input: crate::models::UsableModelInput,
) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_add(&conn, &input)
}

#[tauri::command]
pub fn model_update(
    db: State<DbState>,
    id: i64,
    input: crate::models::UsableModelInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_update(&conn, id, &input)
}

#[tauri::command]
pub fn model_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_delete(&conn, id)
}

// ---------------------------------------------------------------- 價格識別 ---

/// 用已加入來源的模型抽取定價（Key 只在後端使用，不寫 request_logs）。
#[tauri::command]
pub async fn price_extract(
    app: AppHandle,
    provider_id: i64,
    model: String,
    url: Option<String>,
    pasted_text: Option<String>,
    image_base64: Option<String>,
    target_hint: Option<String>,
) -> Result<crate::price_extract::PriceExtractResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        // 短鎖讀憑證 → 放鎖做代抓/LLM 抽取（最多 90s+15s）：不持全局鎖做網路。
        let ctx = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            crate::price_extract::extract_read_ctx(&conn, provider_id, &model)?
        };
        crate::price_extract::extract_run(
            &ctx,
            url.as_deref(),
            pasted_text.as_deref(),
            image_base64.as_deref(),
            target_hint.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------- 訂閱額度 ---

/// 查詢訂閱方案額度（OpenCode Go / Claude CLI / Codex OAuth）：用量百分比 + 重置時間。
/// 只讀本地憑證與官方查詢接口，憑證不回前端；網路在 spawn_blocking，鎖只覆蓋讀取。
#[tauri::command]
pub async fn quota_query_all(app: AppHandle) -> Vec<crate::quota::SubscriptionQuota> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        let providers: Vec<(i64, String, String, String, bool)> = {
            let Ok(conn) = db.conn.lock() else {
                return vec![];
            };
            let Ok(mut stmt) = conn.prepare(
                "SELECT id, name, base_url, api_key, enabled FROM providers
                 ORDER BY priority ASC, id ASC",
            ) else {
                return vec![];
            };
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            });
            match rows {
                Ok(it) => it.filter_map(|r| r.ok()).collect(),
                Err(_) => vec![],
            }
        };
        crate::quota::query_all(&providers)
    })
    .await
    .unwrap_or_default()
}

// ---------------------------------------------------------------- 診斷中心 ---
//
// Phase 1 可觀測性（見 docs/REFACTORING-PLAN.md §5.3 第 0 層）。
// 目的：把「連線錯誤(400)」「codex 對話記錄錯誤」這類只能靠猜的問題，
// 變成可查詢的事實 —— `proxy_trace` 保留每次異常的完整上下文（含 body 指紋
// 與解析失敗時的原始 hex），`provider_stripped_fields` 則記錄各渠道學到的
// 上游能力。兩者皆由 proxy.rs 在請求路徑上被動寫入。

/// 某欄位被多少個渠道的上游拒收。
#[derive(Serialize)]
pub struct StrippedFieldStat {
    pub field: String,
    pub providers: i64,
}

/// 上游狀態碼分佈。
#[derive(Serialize)]
pub struct StatusStat {
    pub status: i64,
    pub count: i64,
}

#[derive(Serialize)]
pub struct TraceSummary {
    pub total: i64,
    pub warn_count: i64,
    pub last_24h: i64,
    /// 有重試過的請求數（即「遇到 400 後剝離重發」的次數）
    pub retried: i64,
    /// 帶有 body hex 的追蹤數 —— 這些就是 body 解析失敗的樣本，
    /// 用於判定 §5.2 那個懸而未決的 400 根因。
    pub with_body_hex: i64,
    pub stripped_fields: Vec<StrippedFieldStat>,
    pub top_status: Vec<StatusStat>,
}

/// 診斷總覽：一次查詢拿到所有計數，供診斷頁開頭顯示。
#[tauri::command]
pub fn trace_summary(db: State<DbState>) -> Result<TraceSummary, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    let since = crate::fsutil::now_ms() - 24 * 60 * 60 * 1000;

    let stripped_fields = trace::stripped_field_summary(&conn)
        .into_iter()
        .map(|(field, providers)| StrippedFieldStat { field, providers })
        .collect();

    let top_status = {
        let mut stmt = conn
            .prepare(
                "SELECT upstream_status, COUNT(*) AS n FROM proxy_trace
                 WHERE upstream_status > 0 GROUP BY upstream_status
                 ORDER BY n DESC, upstream_status ASC LIMIT 10",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(StatusStat {
                    status: r.get(0)?,
                    count: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    Ok(TraceSummary {
        total: count("SELECT COUNT(*) FROM proxy_trace"),
        warn_count: count("SELECT COUNT(*) FROM proxy_trace WHERE trace_level='warn'"),
        last_24h: conn
            .query_row(
                "SELECT COUNT(*) FROM proxy_trace WHERE ts >= ?1",
                [since],
                |r| r.get(0),
            )
            .unwrap_or(0),
        retried: count("SELECT COUNT(*) FROM proxy_trace WHERE retry_count > 0"),
        with_body_hex: count("SELECT COUNT(*) FROM proxy_trace WHERE body_hex <> ''"),
        stripped_fields,
        top_status,
    })
}

/// 最近的請求追蹤（新到舊）。
#[tauri::command]
pub fn trace_list(db: State<DbState>, limit: Option<i64>) -> Result<Vec<trace::TraceRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    trace::list_recent(&conn, limit.unwrap_or(200))
}

/// 只看有問題的追蹤（warn／有剝離欄位／有重試）。
#[tauri::command]
pub fn trace_problems(
    db: State<DbState>,
    limit: Option<i64>,
) -> Result<Vec<trace::TraceRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    trace::list_problems(&conn, limit.unwrap_or(200))
}

#[tauri::command]
pub fn trace_clear(db: State<DbState>) -> Result<usize, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    trace::clear_traces(&conn)
}

/// 某渠道已被記住的上游拒收欄位。
#[tauri::command]
pub fn provider_stripped_list(db: State<DbState>, provider_id: i64) -> Result<Vec<String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(trace::load_stripped(&conn, provider_id))
}

/// 各渠道的上游拒收欄位記憶（含渠道名），供診斷頁一次列出。
/// 用 LEFT JOIN：即使渠道已被刪除而留下孤兒列，也要看得見（而非靜默隱藏），
/// 名稱以空字串回傳，由前端顯示為「已刪除的渠道」。
#[derive(Serialize)]
pub struct ProviderStripped {
    pub provider_id: i64,
    pub provider_name: String,
    pub fields: Vec<String>,
}

#[tauri::command]
pub fn provider_stripped_all(db: State<DbState>) -> Result<Vec<ProviderStripped>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT s.provider_id, COALESCE(p.name, ''), s.field
             FROM provider_stripped_fields s
             LEFT JOIN providers p ON p.id = s.provider_id
             ORDER BY s.provider_id ASC, s.learned_at ASC, s.field ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<ProviderStripped> = Vec::new();
    for (pid, name, field) in rows.flatten() {
        match out.last_mut() {
            Some(last) if last.provider_id == pid => last.fields.push(field),
            _ => out.push(ProviderStripped {
                provider_id: pid,
                provider_name: name,
                fields: vec![field],
            }),
        }
    }
    Ok(out)
}

/// 清除某渠道的拒收記憶：下次請求會重新探測上游能力。
/// 用於「改了渠道設定後想重測」或「誤剝離導致功能缺失」時。
#[tauri::command]
pub fn provider_stripped_clear(db: State<DbState>, provider_id: i64) -> Result<usize, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    trace::clear_stripped(&conn, provider_id)
}


// ---------------------------------------------------------------- 單測 ---

#[cfg(test)]
mod tests {
    //! `commands.rs` 原本沒有任何測試。這裡只測**不需要 Tauri 執行期**的部分：
    //! 純函式與時間邊界。命令本身（`State<DbState>` 等）需要 AppHandle，
    //! 屬於整合測試範疇，不在這裡硬造。

    use super::*;

    // ---- range_start ----

    /// 每個 UI 提供的範圍都要有對應的起點，且落在合理區間。
    #[test]
    fn range_start_covers_every_ui_value() {
        let now = chrono::Local::now().timestamp_millis();
        // 前端 Usage.tsx 的 RANGES 清單
        for (range, days) in [
            ("7d", 7i64),
            ("30d", 30),
            ("90d", 90),
            ("180d", 180),
            ("365d", 365),
        ] {
            let start = range_start(range);
            let expect = now - days * 86400 * 1000;
            // 允許數秒誤差（now 在兩次呼叫之間會走動）
            assert!(
                (start - expect).abs() < 5000,
                "{range}：預期約 {expect}，得到 {start}"
            );
        }
    }

    /// `today` 必須是**本地時區的當天 0 點**，不能是 24 小時前。
    ///
    /// 這是容易寫錯的一條：用 `now - 86400000` 會變成「昨天此刻」，
    /// 使用者選「今天」卻看到跨兩天的資料。
    #[test]
    fn range_start_today_is_local_midnight() {
        let start = range_start("today");
        assert!(start > 0, "today 不該回退成 0");
        let dt = chrono::DateTime::from_timestamp_millis(start)
            .expect("合法時間戳")
            .with_timezone(&chrono::Local);
        use chrono::Timelike;
        assert_eq!(dt.hour(), 0, "應為 0 點：{dt}");
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.second(), 0);
        // 且必須落在今天（與現在同一個日曆日）
        assert_eq!(dt.date_naive(), chrono::Local::now().date_naive());
    }

    /// 未知範圍回傳 0（代表「不限起點」），不可 panic。
    #[test]
    fn range_start_unknown_is_zero() {
        assert_eq!(range_start("all"), 0);
        assert_eq!(range_start(""), 0);
        assert_eq!(range_start("Custom"), 0, "大小寫敏感，不該誤判為 custom");
    }

    // ---- range_bucket_span ----

    /// bucket 粒度只取三個可能值，且與跨度單調對應。
    #[test]
    fn range_bucket_span_picks_granularity() {
        let hour = 3600;
        let day = 86400;
        let week = 7 * 86400;
        let base = 1_700_000_000_000i64;

        // ≤ 2 天 → 小時
        assert_eq!(range_bucket_span(base, Some(base)), hour, "零跨度");
        assert_eq!(range_bucket_span(base, Some(base + 2 * day * 1000)), hour, "剛好 2 天");
        // > 2 天、≤ 186 天 → 日
        assert_eq!(
            range_bucket_span(base, Some(base + 2 * day * 1000 + 1)),
            day,
            "超過 2 天就該換成日"
        );
        assert_eq!(range_bucket_span(base, Some(base + 186 * day * 1000)), day, "剛好 186 天");
        // > 186 天 → 週
        assert_eq!(
            range_bucket_span(base, Some(base + 186 * day * 1000 + 1)),
            week,
            "超過 186 天就該換成週"
        );
    }

    /// `end_ts = None` 時以「現在」為終點（進行中的查詢）。
    #[test]
    fn range_bucket_span_uses_now_when_end_missing() {
        let now = chrono::Local::now().timestamp_millis();
        // 起點在 1 小時前、終點未指定 → 跨度約 1 小時 → 小時粒度
        assert_eq!(range_bucket_span(now - 3600 * 1000, None), 3600);
        // 起點在 100 天前 → 日粒度
        assert_eq!(range_bucket_span(now - 100 * 86400 * 1000, None), 86400);
    }

    /// 跨度上限的寫法必須真的擋得住 366 天，且不誤擋 365 天。
    ///
    /// 註：`resolve_filter` 的訊息寫「上限 365 天」但程式用的是 366 天
    /// （`> 366 * 86400 * 1000`）。這裡把**實際行為**釘住，讓那個落差
    /// 是可見的、而不是靠人記得。
    #[test]
    fn resolve_filter_span_limit_actual_behaviour() {
        let now = chrono::Local::now().timestamp_millis();
        let day = 86400 * 1000;
        // 365 天：必須通過
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 365 * day), Some(now)).is_ok(),
            "365 天應該允許"
        );
        // 367 天：必須擋下
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 367 * day), Some(now)).is_err(),
            "367 天應該被擋"
        );
    }

    /// 空字串的 app／model 要正規化成 None，否則 SQL 會用 `app = ''` 過濾掉全部。
    #[test]
    fn resolve_filter_normalizes_empty_strings() {
        let f = resolve_filter(
            "7d",
            Some(String::new()),
            None,
            Some(String::new()),
            None,
            None,
        )
        .expect("7d 應可用");
        assert_eq!(f.app, None, "空 app 應為 None");
        assert_eq!(f.model, None, "空 model 應為 None");
        // 有值時要保留
        let f = resolve_filter("7d", Some("codex".into()), Some(3), Some("m".into()), None, None)
            .expect("7d 應可用");
        assert_eq!(f.app.as_deref(), Some("codex"));
        assert_eq!(f.model.as_deref(), Some("m"));
        assert_eq!(f.provider_id, Some(3));
    }

    /// 未帶 end_ts 時，end 交給下游（`None`），不要硬塞「現在」——
    /// 硬塞會讓正在進行的查詢無法反映最新資料。
    #[test]
    fn resolve_filter_leaves_open_ended_end_as_none() {
        let now = chrono::Local::now().timestamp_millis();
        let f = resolve_filter("7d", None, None, None, None, None).expect("7d 應可用");
        assert_eq!(f.end_ts, None, "未指定結束時間應保持 None");
        assert!(f.start_ts < now);
        // 明確指定時才填
        let f = resolve_filter("7d", None, None, None, None, Some(now)).expect("應可用");
        assert_eq!(f.end_ts, Some(now));
    }
}
