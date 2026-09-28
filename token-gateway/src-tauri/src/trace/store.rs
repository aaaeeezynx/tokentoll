//! 追蹤表（`proxy_trace`）的寫入、查詢與清理。

use rusqlite::{Connection, Row};
use serde::Serialize;
use std::path::Path;

use crate::fsutil::now_ms;

use super::record::TraceRecord;

/// 寫入一筆追蹤，回傳新行 id。
pub fn insert_trace(conn: &Connection, r: &TraceRecord) -> Result<i64, String> {
    let stripped = serde_json::to_string(&r.stripped_fields).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO proxy_trace
         (ts, trace_level, app, model_raw, in_fmt, target_fmt, trans_kind,
          upstream_status, latency_ms, retry_count, stripped_fields,
          content_length, content_type, body_sha256, body_hex, upstream_error, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        rusqlite::params![
            now_ms(),
            r.level.as_str(),
            r.app,
            r.model_raw,
            r.in_fmt,
            r.target_fmt,
            r.trans_kind,
            r.upstream_status,
            r.latency_ms,
            r.retry_count,
            stripped,
            r.content_length,
            r.content_type,
            r.body_sha256,
            r.body_hex,
            r.upstream_error,
            r.note,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 便利函式：自行開連線寫一筆追蹤。**錯誤一律吞掉** —— 觀測設施不得
/// 影響請求處理，也不得讓網關因寫日誌失敗而回錯。
pub fn log_to(db_path: &Path, r: &TraceRecord) {
    let Ok(conn) = Connection::open(db_path) else {
        return;
    };
    let _ = conn.execute_batch("PRAGMA busy_timeout=5000;");
    let _ = insert_trace(&conn, r);
}

/// 對外顯示用的追蹤列。
#[derive(Debug, Clone, Serialize)]
pub struct TraceRow {
    pub id: i64,
    pub ts: i64,
    pub level: String,
    pub app: String,
    pub model_raw: String,
    pub in_fmt: String,
    pub target_fmt: String,
    pub trans_kind: String,
    pub upstream_status: u16,
    pub latency_ms: i64,
    pub retry_count: i64,
    pub stripped_fields: Vec<String>,
    pub content_length: Option<i64>,
    pub content_type: String,
    pub body_sha256: String,
    pub body_hex: String,
    pub upstream_error: String,
    pub note: String,
}

fn row_to_trace(r: &Row) -> rusqlite::Result<TraceRow> {
    let stripped_json: String = r.get(10)?;
    Ok(TraceRow {
        id: r.get(0)?,
        ts: r.get(1)?,
        level: r.get(2)?,
        app: r.get(3)?,
        model_raw: r.get(4)?,
        in_fmt: r.get(5)?,
        target_fmt: r.get(6)?,
        trans_kind: r.get(7)?,
        upstream_status: r.get(8)?,
        latency_ms: r.get(9)?,
        stripped_fields: serde_json::from_str(&stripped_json).unwrap_or_default(),
        retry_count: r.get(11)?,
        content_length: r.get(12)?,
        content_type: r.get(13)?,
        body_sha256: r.get(14)?,
        body_hex: r.get(15)?,
        upstream_error: r.get(16)?,
        note: r.get(17)?,
    })
}

/// 最近的追蹤列（新到舊）。
pub fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<TraceRow>, String> {
    let limit = limit.clamp(1, 1000);
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, trace_level, app, model_raw, in_fmt, target_fmt, trans_kind,
                    upstream_status, latency_ms, stripped_fields, retry_count,
                    content_length, content_type, body_sha256, body_hex, upstream_error, note
             FROM proxy_trace ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], row_to_trace)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 只取「有實際異常」的追蹤列：warn 層級，或有剝離欄位，或有重試。
/// 這是使用者真正想看的清單。
pub fn list_problems(conn: &Connection, limit: i64) -> Result<Vec<TraceRow>, String> {
    let limit = limit.clamp(1, 1000);
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, trace_level, app, model_raw, in_fmt, target_fmt, trans_kind,
                    upstream_status, latency_ms, stripped_fields, retry_count,
                    content_length, content_type, body_sha256, body_hex, upstream_error, note
             FROM proxy_trace
             WHERE trace_level = 'warn' OR retry_count > 0 OR stripped_fields <> '[]'
             ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], row_to_trace)
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 清空追蹤表，回傳刪除列數。
pub fn clear_traces(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM proxy_trace", [])
        .map_err(|e| e.to_string())
}

/// 保留最近 `keep` 列，其餘刪除（網關啟動時呼叫，避免表無限成長）。
pub fn prune_traces(conn: &Connection, keep: i64) -> Result<usize, String> {
    let keep = keep.max(1);
    conn.execute(
        "DELETE FROM proxy_trace WHERE id NOT IN
         (SELECT id FROM proxy_trace ORDER BY id DESC LIMIT ?1)",
        [keep],
    )
    .map_err(|e| e.to_string())
}
