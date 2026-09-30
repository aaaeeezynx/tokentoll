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
