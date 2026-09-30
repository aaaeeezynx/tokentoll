//! 診斷中心命令（原 `commands.rs` 的「診斷中心」段）。

use serde::Serialize;
use tauri::State;

use crate::db::DbState;
use crate::trace;

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
    /// 學到的「上游協議」記憶：哪個來源的哪個模型在哪種端點上架
    /// （2026-09-30 的自動換手靠它，重啟後仍然有效）。
    pub learned_protocols: Vec<LearnedProtocol>,
}

/// 一筆學到的上游協議（診斷頁顯示「網關記住了什麼」）。
#[derive(Serialize)]
pub struct LearnedProtocol {
    pub provider_id: i64,
    pub provider_name: String,
    pub model: String,
    pub declared_format: String,
    pub actual_format: String,
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
        learned_protocols: trace::protocol_summary(&conn)
            .into_iter()
            .map(
                |(provider_id, provider_name, model, declared_format, actual_format)| {
                    LearnedProtocol {
                        provider_id,
                        provider_name,
                        model,
                        declared_format,
                        actual_format,
                    }
                },
            )
            .collect(),
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

/// 某個來源目前的健康狀態（診斷頁的來源健康區塊）。
#[derive(Serialize)]
pub struct SourceHealth {
    pub provider_id: i64,
    /// 空字串 = 該來源已被刪除（斷路器還有它的紀錄）
    pub provider_name: String,
    /// 該來源目前的協議設定（幫使用者對照「是不是這個來源本來就不通」）
    pub api_format: String,
    pub open: bool,
    pub consecutive_failures: u32,
    pub cooldown_secs: u64,
    pub last_error: String,
    pub secs_since_failure: Option<u64>,
    pub secs_since_success: Option<u64>,
}

/// 各來源的健康狀態（斷路器）。
///
/// **只回「有紀錄」的來源**：完全沒被請求過的來源不會出現 —— 沒有紀錄就是沒有
/// 證據，不該在畫面上假裝它是健康的。被刪掉的來源若還有紀錄也會列出（名稱空白），
/// 與 `provider_stripped_all` 的處理一致：看得見，而不是靜默隱藏。
#[tauri::command]
pub fn source_health(db: State<DbState>) -> Result<Vec<SourceHealth>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, name, api_format FROM providers")
        .map_err(|e| e.to_string())?;
    let providers: std::collections::HashMap<i64, (String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    Ok(crate::proxy::health_registry(&db.path)
        .snapshot()
        .into_iter()
        .map(|h| {
            let (provider_name, api_format) = providers
                .get(&h.provider_id)
                .map(|(n, f)| (n.clone(), f.clone()))
                .unwrap_or_default();
            SourceHealth {
                provider_id: h.provider_id,
                provider_name,
                api_format,
                open: h.open,
                consecutive_failures: h.consecutive_failures,
                cooldown_secs: h.cooldown_secs,
                last_error: h.last_error,
                secs_since_failure: h.secs_since_failure,
                secs_since_success: h.secs_since_success,
            }
        })
        .collect())
}

/// 候選模型清單（故障轉移佇列畫面挑模型用）：模型 + 有幾個來源登記它。
#[tauri::command]
pub fn failover_models(db: State<DbState>) -> Result<Vec<crate::proxy::QueueModel>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::proxy::queue_models(&conn)
}

/// 某個模型的候選來源順序 —— **與請求路徑用的是同一個查詢**
/// （`resolve_model_providers` + `order_by_health`），所以畫面不會與實際行為不一致。
#[tauri::command]
pub fn failover_queue(
    db: State<DbState>,
    model: String,
) -> Result<Vec<crate::proxy::QueueRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let health = crate::proxy::health_registry(&db.path);
    crate::proxy::queue_rows(&conn, &db.path, &model, &health)
}

/// 清除某渠道的拒收記憶：下次請求會重新探測上游能力。
/// 用於「改了渠道設定後想重測」或「誤剝離導致功能缺失」時。
///
/// 2026-09-30 起也一起清掉「學到的上游協議」：兩者都是同一種東西
/// （網關對這個渠道的上游能力記憶），分開清只會讓使用者困惑。
#[tauri::command]
pub fn provider_stripped_clear(db: State<DbState>, provider_id: i64) -> Result<usize, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let n = trace::clear_stripped(&conn, provider_id)?;
    let _ = trace::clear_protocol(&conn, provider_id);
    Ok(n)
}
