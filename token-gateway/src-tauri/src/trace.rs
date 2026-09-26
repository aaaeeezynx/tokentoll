//! Phase 1：網關可觀測性（請求追蹤 + 上游拒收欄位持久化）。
//!
//! 背景（見 `docs/REFACTORING-PLAN.md` §5.2）：本專案原本面對「上游不認某欄位
//! 就回 400」的策略是**事後剝離重試**，這帶來兩個具體缺陷：
//!
//! 1. **記憶不持久**：`proxy.rs` 的 `ProxyCtx.stripped` 只是進程記憶體，
//!    網關每次停止／啟動就歸零 → 每個渠道的第一個請求都要重踩一次 400。
//! 2. **完全不可觀測**：剝離了哪些欄位、上游原文是什麼，只在錯誤時截斷 300 字
//!    透出，事後無法稽核。§5.2 那個無法判定根因的 `body_json_is_some=false`
//!    就是這個缺陷的直接後果。
//!
//! 本模組把兩件事落到 SQLite：
//! - [`remember_stripped`] / [`load_stripped`]：**持久化**的欄位拒收記憶。
//! - [`insert_trace`]：異常請求的完整上下文，含請求體指紋；
//!   body 解析失敗時額外記錄前 [`BODY_HEX_MAX`] bytes 的 hex，用來區分
//!   「真的解析失敗」與「debug 儀器自己弄壞了 body」。
//!
//! **設計約束**
//! - 只在異常時寫入（剝離／4xx／5xx／解析失敗），正常請求不寫，避免日誌洪水。
//! - 金鑰永不落庫：`body_hex` 只取前綴且由呼叫端在解析失敗時才帶入；
//!   正常路徑只存 SHA-256 指紋。
//! - 觀測設施**絕不可**影響請求處理：對外便利函式一律吞掉錯誤。

use rusqlite::{Connection, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::fsutil::now_ms;

/// body 解析失敗時最多記錄多少 bytes 的 hex（512 bytes → 1024 hex 字元）。
pub const BODY_HEX_MAX: usize = 512;

/// 上游錯誤原文入庫上限（存庫比對外顯示寬鬆，UI 端再截斷）。
pub const UPSTREAM_ERR_MAX: usize = 4000;

/// 追蹤層級。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceLevel {
    /// 有剝離欄位或有重試，但最終成功。
    #[default]
    Info,
    /// 上游 4xx/5xx、連線失敗、body 解析失敗，**以及網關自己拒絕的請求**
    /// （見 `proxy::logging::reject`：401／403／413／429／500 與模型不在
    /// 清單內的 400）。這一類沒有上游，`upstream_status` 記的是
    /// **回給客戶端的狀態碼**。
    Warn,
}

impl TraceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceLevel::Info => "info",
            TraceLevel::Warn => "warn",
        }
    }
}

/// 一筆請求追蹤。
///
/// 用結構體 + [`Default`] 而非位置參數，因為呼叫端是 800 行的
/// `proxy_handler`，各分支只關心其中幾個欄位。
#[derive(Debug, Clone, Default)]
pub struct TraceRecord {
    pub level: TraceLevel,
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

impl TraceRecord {
    /// 記錄請求體指紋與長度（**不存明文**）。
    pub fn with_body(mut self, body: &[u8]) -> Self {
        self.content_length = Some(body.len() as i64);
        self.body_sha256 = sha256_hex_bytes(body);
        self
    }

    /// 記錄請求體前綴 hex。僅供 body 解析失敗時使用，用於事後判定根因。
    pub fn with_body_hex(mut self, body: &[u8]) -> Self {
        self.body_hex = hex_prefix(body, BODY_HEX_MAX);
        self.with_body(body)
    }

    /// 記錄上游錯誤原文（自動截斷至 [`UPSTREAM_ERR_MAX`]）並升為 warn。
    pub fn with_upstream_error(mut self, err: &str) -> Self {
        self.upstream_error = truncate_chars(err, UPSTREAM_ERR_MAX);
        self.level = TraceLevel::Warn;
        self
    }

    /// 標為 warn（用於解析失敗等非上游錯誤的異常）。
    pub fn warn(mut self) -> Self {
        self.level = TraceLevel::Warn;
        self
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// 位元組級 SHA-256（`keys::sha256_hex` 只吃 `&str`）。
pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 取前綴 hex（每 byte 兩位小寫）。
pub fn hex_prefix(bytes: &[u8], max: usize) -> String {
    let n = bytes.len().min(max);
    let mut s = String::with_capacity(n * 2);
    for b in &bytes[..n] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ─────────────────────────────────────────── 追蹤寫入 / 讀取 ───

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

// ───────────────────────────────────── 上游拒收欄位的持久化記憶 ───

/// 記住某渠道被上游拒收的欄位（冪等）。
pub fn remember_stripped(conn: &Connection, provider_id: i64, field: &str) -> Result<(), String> {
    if field.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO provider_stripped_fields (provider_id, field, learned_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(provider_id, field) DO NOTHING",
        rusqlite::params![provider_id, field, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 讀取某渠道已知拒收的欄位（依學習時間排序，穩定可重現）。
pub fn load_stripped(conn: &Connection, provider_id: i64) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT field FROM provider_stripped_fields
         WHERE provider_id = ?1 ORDER BY learned_at ASC, field ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([provider_id], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// 清除某渠道的拒收記憶（UI 的「重設上游能力記憶」用）。
pub fn clear_stripped(conn: &Connection, provider_id: i64) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM provider_stripped_fields WHERE provider_id = ?1",
        [provider_id],
    )
    .map_err(|e| e.to_string())
}

/// 掃描出所有「某欄位被多個渠道拒收」的欄位名（診斷頁用）。
pub fn stripped_field_summary(conn: &Connection) -> Vec<(String, i64)> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT field, COUNT(DISTINCT provider_id) AS n
         FROM provider_stripped_fields GROUP BY field ORDER BY n DESC, field ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

// ─────────────────────────────────────────────────────── 單測 ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_and_ensure;

    fn mem() -> Connection {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let c = open_and_ensure(&path).unwrap();
        std::mem::forget(dir); // 連線持有期間不刪檔
        c
    }

    #[test]
    fn hex_prefix_truncates_to_max() {
        let small: Vec<u8> = (0u8..=255).collect();
        assert_eq!(hex_prefix(&small, 4), "00010203");
        // 不足 max → 全取（256 bytes → 512 hex 字元）
        assert_eq!(hex_prefix(&small, 4096).len(), 256 * 2);
        // 超過 max → 截斷
        let big = vec![0xabu8; BODY_HEX_MAX + 10];
        assert_eq!(hex_prefix(&big, BODY_HEX_MAX).len(), BODY_HEX_MAX * 2);
        assert!(hex_prefix(&big, 8).chars().all(|c| c == 'a' || c == 'b'));
        assert_eq!(hex_prefix(b"", 8), "");
    }

    #[test]
    fn sha256_of_bytes_matches_known_vector() {
        // SHA-256("abc") 標準測試向量
        assert_eq!(
            sha256_hex_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn record_helpers_fill_expected_fields() {
        let r = TraceRecord::default()
            .with_body(b"{\"a\":1}")
            .with_upstream_error("boom");
        assert_eq!(r.content_length, Some(7));
        assert_eq!(r.body_sha256.len(), 64);
        assert!(r.body_hex.is_empty(), "with_body 不應記 hex（保護隱私）");
        assert_eq!(r.level, TraceLevel::Warn, "上游錯誤應升為 warn");
        assert_eq!(r.upstream_error, "boom");
    }

    #[test]
    fn with_body_hex_truncates_and_keeps_fingerprint() {
        let body = vec![b'x'; BODY_HEX_MAX + 100];
        let r = TraceRecord::default().with_body_hex(&body);
        assert_eq!(r.body_hex.len(), BODY_HEX_MAX * 2);
        assert_eq!(r.content_length, Some((BODY_HEX_MAX + 100) as i64));
        assert_eq!(r.body_sha256.len(), 64);
    }

    #[test]
    fn stripped_memory_persists_across_connections() {
        // 這是 B2 的回歸測試：記憶必須活過「連線重建」（等同網關重啟）。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let c = open_and_ensure(&path).unwrap();
            remember_stripped(&c, 7, "prompt_cache_key").unwrap();
            remember_stripped(&c, 7, "verbosity").unwrap();
        }
        let c2 = open_and_ensure(&path).unwrap();
        assert_eq!(
            load_stripped(&c2, 7),
            vec!["prompt_cache_key".to_string(), "verbosity".to_string()],
            "重開連線後仍應記得（原實作存在進程記憶體，重啟即歸零）"
        );
        // 別的渠道不受影響
        assert!(load_stripped(&c2, 8).is_empty());
    }

    #[test]
    fn remember_stripped_is_idempotent() {
        let c = mem();
        for _ in 0..5 {
            remember_stripped(&c, 1, "verbosity").unwrap();
        }
        assert_eq!(load_stripped(&c, 1).len(), 1);
        // 空字串不記
        remember_stripped(&c, 1, "").unwrap();
        assert_eq!(load_stripped(&c, 1).len(), 1);
    }

    #[test]
    fn clear_stripped_only_affects_one_provider() {
        let c = mem();
        remember_stripped(&c, 1, "a").unwrap();
        remember_stripped(&c, 2, "b").unwrap();
        assert_eq!(clear_stripped(&c, 1).unwrap(), 1);
        assert!(load_stripped(&c, 1).is_empty());
        assert_eq!(load_stripped(&c, 2), vec!["b".to_string()]);
    }

    #[test]
    fn stripped_field_summary_counts_distinct_providers() {
        let c = mem();
        remember_stripped(&c, 1, "prompt_cache_key").unwrap();
        remember_stripped(&c, 2, "prompt_cache_key").unwrap();
        remember_stripped(&c, 2, "verbosity").unwrap();
        let sum = stripped_field_summary(&c);
        assert_eq!(
            sum,
            vec![("prompt_cache_key".to_string(), 2), ("verbosity".to_string(), 1)]
        );
    }

    #[test]
    fn trace_roundtrip_via_connection() {
        let c = mem();
        let rec = TraceRecord {
            app: "codex".into(),
            model_raw: "gpt-5".into(),
            in_fmt: "responses".into(),
            target_fmt: "openai-chat".into(),
            trans_kind: "responses_to_chat".into(),
            upstream_status: 400,
            latency_ms: 123,
            retry_count: 1,
            stripped_fields: vec!["prompt_cache_key".into()],
            content_type: "application/json".into(),
            ..Default::default()
        }
        .with_body(b"hello")
        .with_upstream_error("Unsupported parameter(s): `prompt_cache_key`");
        let id = insert_trace(&c, &rec).unwrap();
        assert!(id > 0);

        let got = list_recent(&c, 10).unwrap();
        assert_eq!(got.len(), 1);
        let g = &got[0];
        assert_eq!(g.id, id);
        assert_eq!(g.app, "codex");
        assert_eq!(g.level, "warn");
        assert_eq!(g.upstream_status, 400);
        assert_eq!(g.retry_count, 1);
        assert_eq!(g.stripped_fields, vec!["prompt_cache_key".to_string()]);
        assert_eq!(g.content_length, Some(5));
        assert_eq!(g.body_sha256, sha256_hex_bytes(b"hello"));
        assert!(g.body_hex.is_empty());
        assert!(g.upstream_error.contains("Unsupported parameter"));
    }

    #[test]
    fn list_problems_filters_out_clean_requests() {
        let c = mem();
        // 乾淨請求：info、無剝離、無重試 → 不應出現
        insert_trace(
            &c,
            &TraceRecord {
                app: "claude".into(),
                upstream_status: 200,
                ..Default::default()
            },
        )
        .unwrap();
        // 有剝離 → 應出現
        insert_trace(
            &c,
            &TraceRecord {
                stripped_fields: vec!["verbosity".into()],
                upstream_status: 200,
                ..Default::default()
            },
        )
        .unwrap();
        // warn → 應出現
        insert_trace(
            &c,
            &TraceRecord::default().with_upstream_error("bad"),
        )
        .unwrap();
        // 有重試 → 應出現
        insert_trace(
            &c,
            &TraceRecord {
                retry_count: 1,
                upstream_status: 200,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(list_recent(&c, 10).unwrap().len(), 4);
        assert_eq!(list_problems(&c, 10).unwrap().len(), 3);
    }

    #[test]
    fn body_hex_persists_for_parse_failure_diagnosis() {
        // §5.2 的關鍵需求：解析失敗時必須留下原始位元組的 hex。
        let c = mem();
        let raw = b"{\"model\":\"x\"}";
        insert_trace(
            &c,
            &TraceRecord {
                in_fmt: "anthropic".into(),
                upstream_status: 400,
                note: "body 解析失敗".into(),
                ..Default::default()
            }
            .with_body_hex(raw),
        )
        .unwrap();
        let g = &list_recent(&c, 1).unwrap()[0];
        assert_eq!(g.body_hex, hex_prefix(raw, BODY_HEX_MAX));
        assert_eq!(g.content_length, Some(raw.len() as i64));
        assert_eq!(g.note, "body 解析失敗");
    }

    #[test]
    fn prune_and_clear_work() {
        let c = mem();
        for i in 0..10 {
            insert_trace(
                &c,
                &TraceRecord {
                    app: format!("app{i}"),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        assert_eq!(prune_traces(&c, 3).unwrap(), 7);
        assert_eq!(list_recent(&c, 100).unwrap().len(), 3);
        assert_eq!(clear_traces(&c).unwrap(), 3);
        assert!(list_recent(&c, 100).unwrap().is_empty());
    }

    #[test]
    fn list_recent_clamps_limit() {
        let c = mem();
        // 不應 panic，且 limit 被夾在合法範圍
        assert!(list_recent(&c, 0).is_ok());
        assert!(list_recent(&c, 99_999).is_ok());
    }
}
