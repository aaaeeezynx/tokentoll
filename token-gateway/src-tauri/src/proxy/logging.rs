//! 用量解析與落庫、被拒日誌、錯誤回應樣板。
//!
//! `reject` / `log_reject` / `err_json` 三者是網關所有失敗路徑的
//! 共同出口：**每一次拒絕都要留下痕跡**，否則使用者只看到 400，
//! 永遠查不出為什麼。

use super::*;
use crate::translate::{num, read_openai_usage, SseUsage};

/// 從任意響應 JSON 提取用量 + 模型（流式累積取“最後一次”，非流式取唯一一次）。
pub(super) fn extract_usage(v: &serde_json::Value, acc: &mut SseUsage) {
    if let Some(u) = v.get("usage") {
        if u.is_object() {
            read_openai_usage(u, acc);
        }
    }
    if let Some(u) = v.get("message").and_then(|m| m.get("usage")) {
        read_openai_usage(u, acc);
    }
    if let Some(u) = v.get("response").and_then(|r| r.get("usage")) {
        read_openai_usage(u, acc);
    }
    // Google 原生 usageMetadata。thoughts 按輸出計費，併入 out；
    // candidates 缺席時用 total-prompt 回退（如被截斷的 MAX_TOKENS 回包）。
    if let Some(u) = v.get("usageMetadata") {
        if let Some(n) = num(u, "promptTokenCount") {
            acc.in_tok = n;
        }
        let cand = num(u, "candidatesTokenCount");
        let thoughts = num(u, "thoughtsTokenCount");
        if cand.is_some() || thoughts.is_some() {
            acc.out_tok = cand.unwrap_or(0) + thoughts.unwrap_or(0);
        } else if let (Some(t), Some(p)) =
            (num(u, "totalTokenCount"), num(u, "promptTokenCount"))
        {
            acc.out_tok = (t - p).max(0);
        }
        if let Some(n) = num(u, "cachedContentTokenCount") {
            acc.cache_read = n;
        }
    }
    if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
        acc.model = Some(m.to_string());
    }
    if let Some(m) = v
        .get("response")
        .and_then(|r| r.get("model"))
        .and_then(|m| m.as_str())
    {
        acc.model = Some(m.to_string());
    }
}
pub(super) struct SseAcc {
    pub(super) buf: String,
    pub(super) usage: SseUsage,
}
impl SseAcc {
    pub(super) fn new() -> Self {
        Self {
            buf: String::new(),
            usage: SseUsage::default(),
        }
    }

    pub(super) fn feed(&mut self, chunk: &[u8]) {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        // 逐行消費完整行；不足一行的半包殘留等下個 chunk。
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            self.feed_line(line.trim_end_matches(['\r', '\n']));
        }
    }

    /// 流結束時消費殘留行（正常 SSE 以空行結尾時這裡是空操作）。
    pub(super) fn finish(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let rest = std::mem::take(&mut self.buf);
        self.feed_line(rest.trim_end_matches(['\r', '\n']));
    }

    pub(super) fn feed_line(&mut self, t: &str) {
        // 只關心 data: 行；event:/id:/retry: 與空行對用量無用。
        // 注意：單條 usage JSON 在實踐中獨佔一行，無需跨行拼接。
        let Some(d) = t.strip_prefix("data:") else {
            return;
        };
        let d = d.trim_start();
        if d == "[DONE]" || d.is_empty() {
            return;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(d) {
            extract_usage(&v, &mut self.usage);
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_log(
    conn: &rusqlite::Connection,
    ts: i64,
    key_id: Option<i64>,
    app: &str,
    provider_id: Option<i64>,
    model_raw: &str,
    model_norm: &str,
    u: &SseUsage,
    cost: f64,
    latency_ms: i64,
    status: i64,
    is_stream: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO request_logs (ts, key_id, app, provider_id, model_raw, model_norm,
         in_tok, out_tok, cache_read, cache_write, cost_usd, latency_ms, status, is_stream)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        rusqlite::params![
            ts,
            key_id,
            app,
            provider_id,
            model_raw,
            model_norm,
            u.in_tok,
            u.out_tok,
            u.cache_read,
            u.cache_write,
            cost,
            latency_ms,
            status,
            if is_stream { 1 } else { 0 },
        ],
    )?;
    Ok(())
}
#[derive(Debug, Clone, Serialize)]
pub struct LogRow {
    pub id: i64,
    pub ts: i64,
    pub key_id: Option<i64>,
    pub app: String,
    pub provider_id: Option<i64>,
    pub model_raw: String,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost_usd: f64,
    pub latency_ms: i64,
    pub status: i64,
    pub is_stream: bool,
    pub source: String,
}
pub(crate) fn recent_logs(
    conn: &rusqlite::Connection,
    limit: i64,
) -> rusqlite::Result<Vec<LogRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, key_id, app, provider_id, model_raw, in_tok, out_tok,
                cache_read, cache_write, cost_usd, latency_ms, status, is_stream, source
         FROM request_logs ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit.clamp(1, 500)], |r| {
        Ok(LogRow {
            id: r.get(0)?,
            ts: r.get(1)?,
            key_id: r.get(2)?,
            app: r.get(3)?,
            provider_id: r.get(4)?,
            model_raw: r.get(5)?,
            in_tok: r.get(6)?,
            out_tok: r.get(7)?,
            cache_read: r.get(8)?,
            cache_write: r.get(9)?,
            cost_usd: r.get(10)?,
            latency_ms: r.get(11)?,
            status: r.get(12)?,
            is_stream: r.get::<_, i64>(13)? != 0,
            source: r.get(14)?,
        })
    })?;
    rows.collect()
}

// ---------------------------------------------------------------- 轉發 ---
pub(super) fn err_json(status: StatusCode, message: String) -> Response {
    let body = serde_json::json!({"error": {"message": message, "type": "gateway_error"}});
    (status, axum::Json(body)).into_response()
}
/// 拒絕請求：寫一筆被拒日誌，並回傳統一格式的錯誤 JSON。
///
/// 這個「記錄 + 回應」配對原本在 `proxy_handler` 內手寫了 11 次、每次 8–10 行，
/// 是該函式膨脹到 900 行的主因之一（見 §2.1）。集中後每處只留一行。
pub(super) fn reject(
    ctx: &ProxyCtx,
    started: &Instant,
    app: &str,
    model: &str,
    status: StatusCode,
    message: impl Into<String>,
) -> Response {
    log_reject(
        &ctx.db_path,
        app,
        model,
        status.as_u16(),
        started.elapsed().as_millis() as i64,
    );
    err_json(status, message.into())
}
/// 拒絕/失敗請求也落庫（key 未知記 NULL、零 token），否則用量頁完全看不到
/// 被擋掉的流量，除錯只能靠猜。
pub(super) fn log_reject(db_path: &PathBuf, app: &str, model_raw: &str, status: u16, latency_ms: i64) {
    if let Ok(conn) = open_conn(db_path) {
        let norm = normalize_model(model_raw);
        let _ = insert_log(
            &conn,
            now_ms(),
            None,
            app,
            None,
            model_raw,
            &norm,
            &SseUsage::default(),
            0.0,
            latency_ms,
            status as i64,
            false,
        );
    }
}
