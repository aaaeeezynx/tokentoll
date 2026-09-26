//! M3：本地網關代理（`127.0.0.1:<port>`，純透傳）。
//!
//! - 鑑權：`Authorization: Bearer sk-local-…` → 查 `local_keys`（哈希比對、
//!   啟用/過期/配額/模型白名單/應用程式白名單）→ 按 Key 綁定的上游渠道轉發。
//! - 用量：非流式直接讀 JSON `usage`；流式邊轉邊解析 SSE（OpenAI chat /
//!   Responses / Anthropic 三種形狀），請求結束寫 `request_logs`。
//! - 不做格式轉換（M3 透傳；轉換列入 P1）。金鑰只進記憶體，不寫日誌。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Client;
use serde::Serialize;
use tokio::net::TcpListener;

use crate::fsutil::now_ms;
use crate::keys;
use crate::tools::APPS;
use crate::trace::{self, TraceRecord};

mod matrix;

use matrix::{resolve_trans_kind, InFmt, TargetFmt, TransKind};

// ---------------------------------------------------------------- 狀態 ---

/// 上游連線階段上限（秒）。握手卡住時快速失敗，避免佔用請求 10 分鐘。
pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 10;
/// 上游整體請求上限（秒），含串流讀取。
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 600;
/// `proxy_trace` 保留筆數上限（網關啟動時裁剪）。
pub(crate) const TRACE_KEEP: i64 = 5000;

#[derive(Clone)]
pub(crate) struct ProxyCtx {
    pub db_path: PathBuf,
    pub client: Client,
    pub rate: RateLimiter,
}

#[derive(Clone, Default)]
pub(crate) struct RateLimiter {
    inner: Arc<Mutex<HashMap<i64, VecDeque<Instant>>>>,
}

impl RateLimiter {
    /// true = 放行並計數；qpm <= 0 不限流。
    fn check(&self, key_id: i64, qpm: i64) -> bool {
        if qpm <= 0 {
            return true;
        }
        let now = Instant::now();
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let q = map.entry(key_id).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t).as_secs() >= 60) {
            q.pop_front();
        }
        if q.len() as i64 >= qpm {
            return false;
        }
        q.push_back(now);
        true
    }
}

pub struct ProxyState {
    pub inner: Mutex<Option<RunningProxy>>,
}

impl Default for ProxyState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

pub struct RunningProxy {
    pub port: u16,
    pub started_at: i64,
    pub handle: tokio::task::JoinHandle<()>,
}

/// C 方案：啟動前先試綁，衝突時給出可操作的錯誤（常見佔用者：cc-switch 代理）。
pub(crate) fn check_port(port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("連接埠不能為 0".to_string());
    }
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Err(format!(
            "連接埠 {port} 已被佔用（本機可能有 cc-switch 或其他代理在執行）：請在設定中更換連接埠，或停用對方後再啟動"
        )),
        Err(e) => Err(format!("連接埠 {port} 不可用：{e}")),
    }
}

fn open_conn(db_path: &PathBuf) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

// ---------------------------------------------------------------- 用量 ---
// 用量結構與 OpenAI 讀取邏輯統一放在 translate（翻譯與代理共用）。

use crate::translate::{num, read_openai_usage, SseUsage};

/// 從任意響應 JSON 提取用量 + 模型（流式累積取“最後一次”，非流式取唯一一次）。
fn extract_usage(v: &serde_json::Value, acc: &mut SseUsage) {
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

struct SseAcc {
    buf: String,
    usage: SseUsage,
}

impl SseAcc {
    fn new() -> Self {
        Self {
            buf: String::new(),
            usage: SseUsage::default(),
        }
    }

    fn feed(&mut self, chunk: &[u8]) {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        // 逐行消費完整行；不足一行的半包殘留等下個 chunk。
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            self.feed_line(line.trim_end_matches(['\r', '\n']));
        }
    }

    /// 流結束時消費殘留行（正常 SSE 以空行結尾時這裡是空操作）。
    fn finish(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let rest = std::mem::take(&mut self.buf);
        self.feed_line(rest.trim_end_matches(['\r', '\n']));
    }

    fn feed_line(&mut self, t: &str) {
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

/// 模型名歸一化（與 M2 定價文檔一致）：小寫、去 `/` 前綴、去 `:` 後綴與 `[1m]` 尾綴，
/// 再剝最多 3 段版本/日期/渠道尾綴（如 `-ga-260731`、`-2024-07-18`、`-fk-hs`），
/// 使 `deepseek-v4-flash-ga-260731` 對上定價 `deepseek-v4-flash`。
/// 保守策略：只剝完整日期（YYYY-MM-DD / YYYYMMDD / YYMMDD）、vN 版本、`ga`、
/// `fk-*` 渠道標籤；`kimi-k2-0905` 的 MMDD、`gpt-5`、`llama-3` 一律保留。
pub(crate) fn normalize_model(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    if let Some(i) = s.rfind('/') {
        s = s[i + 1..].to_string();
    }
    if let Some(i) = s.find(':') {
        s.truncate(i);
    }
    if let Some(i) = s.find('[') {
        s.truncate(i);
    }
    s = s.replace('@', "-");
    // 整段日期尾綴（-YYYY-MM-DD / -YYYYMMDD / -YYMMDD），ASCII 安全切片
    fn cut_full_date(s: &mut String) -> bool {
        let b = s.as_bytes();
        let n = s.len();
        let dig = |mut r: std::ops::Range<usize>| r.all(|i| b.get(i).is_some_and(|c| c.is_ascii_digit()));
        if n > 11 && b[n - 11] == b'-' && dig(n - 10..n - 6) && b[n - 6] == b'-'
            && dig(n - 5..n - 3) && b[n - 3] == b'-' && dig(n - 2..n)
        {
            s.truncate(n - 11);
            return true;
        }
        if n > 9 && b[n - 9] == b'-' && dig(n - 8..n) {
            s.truncate(n - 9);
            return true;
        }
        if n > 7 && b[n - 7] == b'-' && dig(n - 6..n) {
            s.truncate(n - 7);
            return true;
        }
        false
    }
    for _ in 0..3 {
        if cut_full_date(&mut s) {
            continue;
        }
        let Some(i) = s.rfind('-') else {
            break;
        };
        if i == 0 {
            break;
        }
        let tail = &s[i + 1..];
        let strip = tail.eq_ignore_ascii_case("ga")
            || (tail.starts_with("fk-") && tail.len() > 3)
            || (tail.starts_with('v')
                && tail.len() > 1
                && tail[1..].bytes().all(|b| b.is_ascii_digit() || b == b'.')
                && tail[1..].bytes().any(|b| b.is_ascii_digit()));
        if strip {
            s.truncate(i);
        } else {
            break;
        }
    }
    s
}

/// 從 UA 推斷來源應用程式；允許客戶端用 `X-TG-App` 顯式聲明（取值為 app id）。
pub(crate) fn infer_app(headers: &HeaderMap) -> String {
    if let Some(v) = headers
        .get("x-tg-app")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
    {
        if APPS.iter().any(|(a, _)| *a == v) {
            return v.to_string();
        }
    }
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    for pat in [
        "codex",
        "opencode",
        "hermes",
        "dsh",
        "deepseek",
        "cursor",
        "antigravity",
    ] {
        if ua.contains(pat) {
            return if pat == "deepseek" { "dsh".into() } else { pat.into() };
        }
    }
    if ua.contains("claude") || ua.contains("anthropic") {
        return "claude".into();
    }
    "unknown".into()
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

fn err_json(status: StatusCode, message: String) -> Response {
    let body = serde_json::json!({"error": {"message": message, "type": "gateway_error"}});
    (status, axum::Json(body)).into_response()
}

/// 拒絕請求：寫一筆被拒日誌，並回傳統一格式的錯誤 JSON。
///
/// 這個「記錄 + 回應」配對原本在 `proxy_handler` 內手寫了 11 次、每次 8–10 行，
/// 是該函式膨脹到 900 行的主因之一（見 §2.1）。集中後每處只留一行。
fn reject(
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
fn log_reject(db_path: &PathBuf, app: &str, model_raw: &str, status: u16, latency_ms: i64) {
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

fn bearer(headers: &HeaderMap) -> Option<String> {
    // 優先 Authorization: Bearer（OpenAI / Codex / dsh）
    if let Some(v) = headers.get("authorization") {
        if let Ok(s) = v.to_str() {
            if let Some(k) = s
                .strip_prefix("Bearer ")
                .or_else(|| s.strip_prefix("bearer "))
            {
                return Some(k.trim().to_string());
            }
        }
    }
    // 備選：x-api-key（Claude Code / Anthropic SDK）
    if let Some(v) = headers.get("x-api-key") {
        if let Ok(s) = v.to_str() {
            let k = s.trim();
            if !k.is_empty() {
                return Some(k.to_string());
            }
        }
    }
    None
}

/// 拼接上游 URL：provider 基址已帶 `/v1` 時，去掉入站路徑重複的首段。
fn join_upstream(base: &str, incoming_path: &str) -> String {
    let base = base.trim_end_matches('/');
    let base_last = base.rsplit('/').next().unwrap_or("");
    let mut segs: Vec<&str> = incoming_path.split('/').filter(|s| !s.is_empty()).collect();
    if !segs.is_empty() && segs[0].eq_ignore_ascii_case(base_last) && !base_last.is_empty() {
        segs.remove(0);
    }
    if segs.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{}", segs.join("/"))
    }
}

fn is_hop_header(name: &str) -> bool {
    matches!(
        name,
        "host" | "connection" | "transfer-encoding" | "content-length"
            | "accept-encoding" | "authorization" | "x-goog-api-key"
            | "x-api-key" | "anthropic-version" | "proxy-authenticate"
            | "proxy-authorization" | "te" | "trailer" | "upgrade"
    )
}

/// 是否注入 stream_options 索取用量：僅 OpenAI chat（含翻譯後）。
/// Responses / Anthropic / Gemini 會因未知欄位被上游 400。
fn should_inject_usage(translated: bool, incoming_path: &str) -> bool {
    translated || incoming_path.contains("/chat/completions")
}

/// 從拒收錯誤訊息提取欄位名，支援兩種口徑：
/// - New-API 系：`unknown field "xxx"`（單個；上游回包是 JSON，訊息裡的引號是
///   轉義的 `\"`，因此按 token 抓取而非找引號）
/// - OpenRouter：`Unsupported parameter(s): 'xxx'`（可能多個：`'a', 'b'`；
///   Codex 的 prompt_cache_key 經此口徑被拒，2026-09-24 實例）
fn parse_unknown_fields(msg: &str) -> Vec<String> {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if let Some(i) = msg.find("unknown field") {
        let rest = &msg[i + "unknown field".len()..];
        let start = match rest.find(is_name) {
            Some(s) => s,
            None => return vec![],
        };
        let end = rest[start..]
            .find(|c: char| !is_name(c))
            .map(|e| start + e)
            .unwrap_or(rest.len());
        let name = &rest[start..end];
        if name.is_empty() || name.len() > 64 {
            return vec![];
        }
        return vec![name.to_string()];
    }
    if let Some(i) = msg.find("Unsupported parameter") {
        let mut out: Vec<String> = vec![];
        let mut rest = &msg[i..];
        while let Some(q) = rest.find(['\'', '`', '"']) {
            let quote = rest.as_bytes()[q] as char;
            let after = &rest[q + quote.len_utf8()..];
            let Some(e) = after.find(quote) else {
                break;
            };
            let name = &after[..e];
            if !name.is_empty()
                && name.len() <= 64
                && name.chars().all(is_name)
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_string());
            }
            rest = &after[e + quote.len_utf8()..];
        }
        return out;
    }
    vec![]
}

/// 解析請求/轉發體 JSON，容忍 UTF-8 BOM（見 body_json 處注記）。
fn parse_body_json(body: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(body.strip_prefix(b"\xef\xbb\xbf").unwrap_or(body)).ok()
}

/// 移除 JSON 頂層欄位；若不存在再試 `text.<field>`（Codex 把 verbosity
/// 放在 text 裡，置空後整個 text 一併移除）。
/// 嚴格上游拒收的新欄位時用。任一處命中即返回新 body。
fn strip_json_field(body: &[u8], field: &str) -> Option<Vec<u8>> {
    let mut v: serde_json::Value = parse_body_json(body)?;
    let obj = v.as_object_mut()?;
    if obj.remove(field).is_some() {
        return serde_json::to_vec(&v).ok();
    }
    if let Some(text) = obj.get_mut("text") {
        if let Some(tobj) = text.as_object_mut() {
            if tobj.remove(field).is_some() {
                if tobj.is_empty() {
                    obj.remove("text");
                }
                return serde_json::to_vec(&v).ok();
            }
        }
    }
    None
}

/// 從請求體中過濾 `tools` 陣列，只保留上游支援的 tool type（`function`）。
/// Codex 會發 `type: "custom"` 等工具，DeepSeek 等上游不認會 400。
fn strip_unsupported_tools(body: &[u8]) -> Option<Vec<u8>> {
    let mut v: serde_json::Value = parse_body_json(body)?;
    // 先過濾 tools，限制可變借用作用域
    let (new_len, original_len) = {
        let obj = v.as_object_mut()?;
        let tools = obj.get_mut("tools")?.as_array_mut()?;
        let orig = tools.len();
        tools.retain(|t| {
            t.get("type")
                .and_then(|tt| tt.as_str())
                .is_some_and(|tt| tt == "function")
        });
        (tools.len(), orig)
    };
    if new_len == 0 {
        let obj = v.as_object_mut().unwrap();
        obj.remove("tools");
        obj.remove("tool_choice");
        return serde_json::to_vec(&v).ok();
    }
    if new_len == original_len {
        return None;
    }
    // 檢查 tool_choice 引用的 tool 是否還在（只讀借用，不與 tools 衝突）
    let tc_name: Option<String> = v
        .get("tool_choice")
        .and_then(|tc| tc.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    if let Some(name) = tc_name {
        let still_has = v["tools"]
            .as_array()
            .map(|tools| {
                tools.iter().any(|t| {
                    t.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        == Some(&name)
                })
            })
            .unwrap_or(false);
        if !still_has {
            v.as_object_mut().unwrap().remove("tool_choice");
        }
    }
    serde_json::to_vec(&v).ok()
}

/// 遞迴移除所有 `encrypted_content` 欄位（含 reasoning item 內）。
///
/// 目的：Codex 在 ChatGPT 授權模式下產生的推理簽章只對 OpenAI 本身有效，
/// 送到任何第三方 responses 端點都會被 400 拒收：
/// `reasoning \`encrypted_content\` was not issued to this caller`。
///
/// 只在**直通**路徑需要（兩條翻譯路徑都已是白名單重建 body，天然不含此欄位）。
/// 回傳 `None` 表示未命中 —— 原文一字不動。
fn strip_encrypted_content(body: &[u8]) -> Option<Vec<u8>> {
    // 位元組快掃短路：絕大多數請求不含此欄位，避免每次多一次完整 JSON parse。
    const NEEDLE: &[u8] = b"encrypted_content";
    if !body.windows(NEEDLE.len()).any(|w| w == NEEDLE) {
        return None;
    }
    fn walk(v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(o) => {
                let mut hit = o.remove("encrypted_content").is_some();
                for (_, child) in o.iter_mut() {
                    // 不用 `|=` 短路：即使已命中也要走完全樹，移除所有副本
                    if walk(child) {
                        hit = true;
                    }
                }
                hit
            }
            serde_json::Value::Array(a) => {
                let mut hit = false;
                for c in a.iter_mut() {
                    if walk(c) {
                        hit = true;
                    }
                }
                hit
            }
            _ => false,
        }
    }
    let mut v: serde_json::Value = parse_body_json(body)?;
    if walk(&mut v) {
        serde_json::to_vec(&v).ok()
    } else {
        None
    }
}

/// 直通 chat 請求的歷史配對修復（舊 session 續跑常見孤兒 tool_calls）。
/// 僅處理 OpenAI chat 格式且未經翻譯的請求體；有效歷史返回 None（原文一字不動）。
fn sanitize_passthrough_chat_body(
    body: &[u8],
    in_fmt: InFmt,
    translated: bool,
) -> Option<Vec<u8>> {
    if translated || in_fmt != InFmt::OpenAi {
        return None;
    }
    let v: serde_json::Value = parse_body_json(body)?;
    let arr = v.get("messages")?.as_array()?.clone();
    let fixed = crate::translate::sanitize_chat_messages(arr.clone());
    if fixed == arr {
        return None;
    }
    let mut obj = v.as_object()?.clone();
    obj.insert(
        "messages".to_string(),
        serde_json::Value::Array(fixed),
    );
    serde_json::to_vec(&serde_json::Value::Object(obj)).ok()
}

/// 上游錯誤原文透出（截斷，便於排查；不含密鑰）。
fn upstream_err_text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let t = s.trim();
    if t.is_empty() {
        return "上游返回錯誤（空回包）".to_string();
    }
    const MAX: usize = 300;
    if t.len() > MAX {
        format!("{}…", &t[..MAX])
    } else {
        t.to_string()
    }
}

/// 去掉 query 中的 `key` 參數（Google 式鑑權殘留，避免客戶端 key 上送）。
fn strip_key_param(query: Option<&str>) -> Option<String> {
    let q = query?;
    let kept: Vec<&str> = q
        .split('&')
        .filter(|kv| {
            let k = kv.split('=').next().unwrap_or("");
            !k.eq_ignore_ascii_case("key")
        })
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join("&"))
    }
}

/// Gemini 原生把模型寫在 URL（`/v1beta/models/<id>:generateContent`），
/// 請求體無 model 欄位時從路徑回填，否則日誌與白名單無模型可用。
fn model_from_path(path: &str) -> Option<String> {
    let last = path.rsplit('/').next()?;
    let (id, _) = last.split_once(':')?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// 跨來源路由命中行：改走來源的連接資訊。
struct ReroutedProvider {
    id: i64,
    base_url: String,
    api_key: String,
    scheme: String,
    format: String,
}

/// 組裝上游 URL 與轉發標頭，回傳 `(url, headers)`。
///
/// - 翻譯路徑固定打 `/chat/completions`；直通保留原始路徑（`join_upstream`
///   負責去重 `/v1` 之類的重複前綴）。查詢字串會剝掉 `key=` 再附加。
/// - 標頭：先複製客戶端標頭（略過 hop-by-hop），再依渠道 `auth_scheme`
///   注入鑑權 —— `goog-key` 用 `x-goog-api-key`、`anthropic` 用 `x-api-key`
///   + 版本頭、其餘用 `Bearer`。
fn build_upstream_target(
    parts: &axum::http::request::Parts,
    authed: &keys::AuthedKey,
    translated: bool,
    app: &str,
) -> (String, Vec<(String, String)>) {
    let mut url = if translated {
        format!(
            "{}/chat/completions",
            authed.provider_base_url.trim_end_matches('/')
        )
    } else {
        join_upstream(&authed.provider_base_url, parts.uri.path())
    };
    if let Some(q) = strip_key_param(parts.uri.query()) {
        url.push('?');
        url.push_str(&q);
    }

    let mut fwd: Vec<(String, String)> = Vec::new();
    for (k, v) in parts.headers.iter() {
        let name = k.as_str();
        if is_hop_header(name) {
            continue;
        }
        if let Ok(val) = v.to_str() {
            fwd.push((name.to_string(), val.to_string()));
        }
    }
    let pkey = authed.provider_api_key.as_str();
    match authed.provider_auth_scheme.as_str() {
        "goog-key" => {
            // Google 原生：x-goog-api-key，不用 Bearer
            if !pkey.is_empty() {
                fwd.push(("x-goog-api-key".to_string(), pkey.to_string()));
            }
        }
        "anthropic" => {
            // Anthropic 官方：x-api-key + 版本頭
            if !pkey.is_empty() {
                fwd.push(("x-api-key".to_string(), pkey.to_string()));
            }
            fwd.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        _ => {
            if !pkey.is_empty() {
                fwd.push(("authorization".to_string(), format!("Bearer {pkey}")));
            }
        }
    }

    // OpenCode Zen/Go 要求 session 標頭才能有效路由（缺失回 400 MissingSessionID）。
    // 客戶端（hermes/opencode/claude 等）沒帶時，注入穩定的合成 id：同一本地 Key
    // 固定同一 id → 上游路由與前綴快取穩定；Codex 自帶 session-id 時原樣透傳。
    if authed.provider_base_url.contains("opencode.ai") {
        const SESSION_HEADS: [&str; 6] = [
            "x-opencode-session",
            "session-id",
            "x-session-id",
            "thread-id",
            "x-client-request-id",
            "x-grok-session-id",
        ];
        let has_session = fwd
            .iter()
            .any(|(k, _)| SESSION_HEADS.iter().any(|h| k.eq_ignore_ascii_case(h)));
        if !has_session {
            let who = if app.is_empty() { "unknown" } else { app };
            let sid = format!("tg-{who}-{}", authed.id);
            fwd.push(("x-opencode-session".to_string(), sid.clone()));
            fwd.push(("session-id".to_string(), sid));
        }
    }
    // 注：X-TG-App 是網關內務頭，如需隱藏可在此剝離；當前選擇透傳以便上游觀測。
    (url, fwd)
}

/// 取渠道顯示名（僅用於錯誤訊息；查不到時給可讀的佔位字串）。
fn provider_name(conn: &rusqlite::Connection, provider_id: i64) -> String {
    conn.query_row("SELECT name FROM providers WHERE id=?1", [provider_id], |r| {
        r.get(0)
    })
    .unwrap_or_else(|_| "（未知來源）".to_string())
}

/// 跨來源模型路由：在其他啟用來源的可用模型表中找 display/actual 命中
///（排除 Key 綁定來源，按來源優先級、id 取第一個）。
fn resolve_model_provider(
    conn: &rusqlite::Connection,
    model: &str,
    exclude_pid: i64,
) -> Option<ReroutedProvider> {
    conn.query_row(
        "SELECT p.id, p.base_url, p.api_key, p.auth_scheme, p.api_format
         FROM provider_models m JOIN providers p ON p.id = m.provider_id
         WHERE m.enabled = 1 AND p.enabled = 1 AND p.id != ?2
           AND (lower(m.display_name) = lower(?1) OR lower(m.actual_model) = lower(?1))
         ORDER BY p.priority ASC, p.id ASC LIMIT 1",
        rusqlite::params![model, exclude_pid],
        |r| {
            Ok(ReroutedProvider {
                id: r.get(0)?,
                base_url: r.get(1)?,
                api_key: r.get(2)?,
                scheme: r.get(3)?,
                format: r.get(4)?,
            })
        },
    )
    .ok()
}

/// 處理一行上游 SSE（Responses 翻譯分支）：回傳（要送出的事件, 是否停止讀流）。
/// - `[DONE]` → 標記正常收尾並停止；
/// - `{"error": …}` → 發 `response.failed` 並停止（不再靜默當 completed）；
/// - 其餘 → chat chunk → Responses 事件。
fn responses_line_events(
    t: &str,
    tstate: &mut crate::translate::ResponsesStreamState,
    echo: &str,
    acc: &mut SseUsage,
    customs: &[String],
) -> (Vec<String>, bool) {
    let Some(d) = t.strip_prefix("data:") else {
        return (vec![], false);
    };
    let d = d.trim_start();
    if d.is_empty() {
        return (vec![], false);
    }
    if d == "[DONE]" {
        tstate.note_done();
        return (vec![], true);
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(d) else {
        return (vec![], false);
    };
    if let Some(err) = v.get("error") {
        let err = if err.is_null() { v.clone() } else { err.clone() };
        return (crate::translate::responses_stream_fail(tstate, echo, &err), true);
    }
    (
        crate::translate::chat_chunk_to_responses(tstate, &v, echo, acc, customs),
        false,
    )
}

/// SSE 轉送的統一骨架：逐行讀上游、交給 `on_line` 轉譯、結束時由 `on_finish`
/// 補收尾事件，最後把狀態交還呼叫端（供寫用量日誌）。
///
/// 三個串流分支原本各自手寫這套 buffer/drain/收流邏輯（各約 60–70 行），
/// 且尾行處理與斷線處理的細節已經開始分歧。這裡統一：`on_line` 回傳
/// `(要送出的內容, 是否收流)`，非 `data:` 行或無法解析的行只要回空 vec 即可。
async fn relay_sse<S, F, G>(
    upstream: reqwest::Response,
    tx: &tokio::sync::mpsc::Sender<Result<axum::body::Bytes, axum::Error>>,
    mut state: S,
    mut on_line: F,
    mut on_finish: G,
) -> S
where
    F: FnMut(&mut S, &str) -> (Vec<String>, bool),
    G: FnMut(&mut S) -> Vec<String>,
{
    use tokio_stream::StreamExt;
    let mut buf = String::new();
    let mut closed = false;
    let mut stream = upstream.bytes_stream();
    'outer: while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(nl) = buf.find('\n') {
                    let line: String = buf.drain(..=nl).collect();
                    let t = line.trim_end_matches(['\r', '\n']);
                    let (outs, stop) = on_line(&mut state, t);
                    for o in outs {
                        if tx.send(Ok(axum::body::Bytes::from(o))).await.is_err() {
                            closed = true;
                            break 'outer;
                        }
                    }
                    if stop {
                        closed = true;
                        break 'outer;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Err(axum::Error::new(e))).await;
                closed = true;
                break;
            }
        }
    }
    // 尾端沒有換行的殘留行（正常 SSE 以空行結尾，這裡通常空操作）
    if !closed {
        let rest = buf.trim_end_matches(['\r', '\n']).to_string();
        if !rest.is_empty() {
            let (outs, _) = on_line(&mut state, &rest);
            for o in outs {
                let _ = tx.send(Ok(axum::body::Bytes::from(o))).await;
            }
        }
    }
    for o in on_finish(&mut state) {
        let _ = tx.send(Ok(axum::body::Bytes::from(o))).await;
    }
    state
}

/// 串流結束後寫用量日誌所需的固定脈絡（三個串流分支共用）。
struct StreamLog<'a> {
    db_path: &'a PathBuf,
    app: &'a str,
    provider_id: i64,
    key_id: Option<i64>,
    latency_ms: i64,
    status: u16,
}

impl<'a> StreamLog<'a> {
    fn new(
        db_path: &'a PathBuf,
        app: &'a str,
        provider_id: i64,
        key_id: Option<i64>,
        latency_ms: i64,
        status: u16,
    ) -> Self {
        Self {
            db_path,
            app,
            provider_id,
            key_id,
            latency_ms,
            status,
        }
    }

    /// 寫一筆串流請求的用量日誌並累加 Key 額度。
    /// （原本三個分支各手寫約 20 行的同款程式碼，只有 model/usage 來源不同。）
    fn write(&self, model: &str, acc: &SseUsage) {
        let norm = normalize_model(model);
        let Ok(conn) = open_conn(self.db_path) else {
            return;
        };
        let ts = now_ms();
        let rates = crate::usage::resolve_rates(&conn, Some(self.provider_id), &norm, ts);
        let cost = crate::usage::rate_cost(
            &rates,
            acc.in_tok,
            acc.out_tok,
            acc.cache_read,
            acc.cache_write,
        );
        let _ = insert_log(
            &conn,
            ts,
            self.key_id,
            self.app,
            Some(self.provider_id),
            model,
            &norm,
            acc,
            cost,
            self.latency_ms,
            self.status as i64,
            true,
        );
        if let Some(kid) = self.key_id {
            let _ = keys::add_used(&conn, kid, acc.total());
        }
    }
}

/// 把 mpsc 接收端包成 SSE 響應；建 body 失敗時回 500。
fn sse_response(
    builder: axum::http::response::Builder,
    rx: tokio::sync::mpsc::Receiver<Result<axum::body::Bytes, axum::Error>>,
) -> Response {
    let stream_body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));
    builder
        .body(stream_body)
        .unwrap_or_else(|_| err_json(StatusCode::INTERNAL_SERVER_ERROR, "構造響應失敗".into()))
}

/// Responses 翻譯分支的逐行轉譯狀態。
#[derive(Default)]
struct ResponsesRelay {
    tstate: crate::translate::ResponsesStreamState,
    acc: SseUsage,
}

/// Anthropic 翻譯分支的逐行轉譯狀態。
#[derive(Default)]
struct AnthropicRelay {
    tstate: crate::translate::AnthropicStreamState,
    acc: SseUsage,
}

/// 翻譯決策（矩陣判定結果中，翻譯階段真正需要的部分）。
#[derive(Debug, Clone, Copy)]
struct TransSpec {
    kind: TransKind,
    /// 是否需要翻譯（`kind` 不是 None，且渠道協議不是原生同型）。
    translated: bool,
}

/// 要送往上游的請求體。
struct ForwardBody {
    bytes: Vec<u8>,
    /// 送往上游的實際模型名（翻譯或直通改寫時）；未改寫則為空字串。
    model: String,
}

/// `translate_forward_body` 的結果。
enum BodyPrep {
    Ready(ForwardBody),
    /// 需要翻譯，但請求體不是合法 JSON —— 呼叫端負責留痕與回應。
    Unparsable,
}

/// 把客戶端請求體轉成要送上游的位元組。
///
/// - 翻譯路徑（Anthropic／Responses → chat）：以白名單重建 body，
///   同時把 model 由顯示名換成 actual。
/// - 直通路徑：只把 body 內既有的 `model` 字串由顯示名改寫成 actual
///   （無映射則原樣不動；Gemini 原生等無 model 的 body 完全不碰）。
/// - 最後若為 OpenAI chat 的串流請求，補 `stream_options.include_usage` ——
///   Responses／Anthropic／Gemini 加這個欄位會被上游 400，故以
///   `want_usage_opt` 區分。
fn translate_forward_body(
    conn: &rusqlite::Connection,
    provider_id: i64,
    raw: &[u8],
    body_json: &Option<serde_json::Value>,
    model_raw: &str,
    fmt: TransSpec,
    want_usage_opt: bool,
) -> BodyPrep {
    let mut fwd_value = body_json.clone();
    let mut translated_model = String::new();
    let mut rewritten = false;

    if fmt.translated {
        let Some(v) = fwd_value.as_ref().cloned() else {
            return BodyPrep::Unparsable;
        };
        let actual = crate::models::resolve_actual(conn, provider_id, model_raw);
        fwd_value = Some(if fmt.kind == TransKind::ResponsesToChat {
            crate::translate::responses_to_openai(&v, &actual)
        } else {
            crate::translate::anthropic_to_openai(&v, &actual)
        });
        translated_model = actual;
    } else if !model_raw.is_empty() {
        // 直通改寫：僅當請求體本來就帶 model 字串才動
        if let Some(obj) = fwd_value.as_mut().and_then(|v| v.as_object_mut()) {
            if obj.get("model").and_then(|m| m.as_str()).is_some() {
                let actual = crate::models::resolve_actual(conn, provider_id, model_raw);
                if !actual.is_empty() && actual != model_raw {
                    obj.insert(
                        "model".to_string(),
                        serde_json::Value::String(actual.clone()),
                    );
                    translated_model = actual;
                    rewritten = true;
                }
            }
        }
    }

    let mut bytes = if fmt.translated || rewritten {
        serde_json::to_vec(fwd_value.as_ref().expect("forward body"))
            .unwrap_or_else(|_| raw.to_vec())
    } else {
        raw.to_vec()
    };
    // 串流強制索取用量（僅 OpenAI chat 需要；見上方說明）
    if want_usage_opt {
        if let Some(v) = fwd_value.as_ref() {
            if v.get("stream").and_then(|s| s.as_bool()).unwrap_or(false)
                && v.get("stream_options").is_none()
            {
                let mut owned = v.clone();
                if let Some(obj) = owned.as_object_mut() {
                    obj.insert(
                        "stream_options".to_string(),
                        serde_json::json!({"include_usage": true}),
                    );
                    bytes = serde_json::to_vec(&owned).unwrap_or(bytes);
                }
            }
        }
    }
    BodyPrep::Ready(ForwardBody {
        bytes,
        model: translated_model,
    })
}

/// 依序套用所有「上游已知拒收」的剝離：
/// ① 該渠道學到的拒收欄位（SQLite 持久化記憶，見 §3 B2）
/// ② OpenAI 專屬推理簽章 `encrypted_content`（§3 B1）
/// ③ 上游不支援的 tool types（Codex `custom` → DeepSeek 400）
/// ④ 直通 chat 的歷史配對修復（舊 session 孤兒 tool_calls；有效歷史不動）
///
/// 每個步驟都是「有改才換、沒改不動」，所以全部 no-op 時 body 原樣送出。
fn strip_for_upstream(
    conn: &rusqlite::Connection,
    provider_id: i64,
    mut body: Vec<u8>,
    in_fmt: InFmt,
    translated: bool,
) -> Vec<u8> {
    for f in trace::load_stripped(conn, provider_id) {
        if let Some(nb) = strip_json_field(&body, &f) {
            body = nb;
        }
    }
    if let Some(stripped) = strip_encrypted_content(&body) {
        body = stripped;
    }
    if !translated {
        if let Some(stripped) = strip_unsupported_tools(&body) {
            body = stripped;
        }
        if let Some(fixed) = sanitize_passthrough_chat_body(&body, in_fmt, translated) {
            body = fixed;
        }
    }
    body
}

/// 上游連線資訊，並負責實際發送。
///（把 client／method／url／headers 綁成一個物件，免得 send 變成一長串參數。）
struct Upstream<'a> {
    client: &'a reqwest::Client,
    method: reqwest::Method,
    url: String,
    headers: Vec<(String, String)>,
}

impl Upstream<'_> {
    async fn send(&self, body: Vec<u8>) -> Result<reqwest::Response, reqwest::Error> {
        let mut b = self.client.request(self.method.clone(), &self.url);
        for (k, v) in self.headers.iter() {
            b = b.header(k, v);
        }
        // 強制 identity：上游若壓縮，我們得先解壓才能解析 SSE。
        b = b.header("accept-encoding", "identity");
        b.body(body).send().await
    }
}

/// 400 重試階段所需的請求脈絡（留痕與拒絕都要用）。
struct RetryCtx<'a> {
    ctx: &'a ProxyCtx,
    started: &'a Instant,
    app: &'a str,
    model_raw: &'a str,
    content_type: &'a str,
    in_fmt: InFmt,
    target_fmt: TargetFmt,
    kind: TransKind,
}

impl RetryCtx<'_> {
    /// 建立一筆本請求的追蹤記錄（尚未寫入）。
    /// `retry_count` 由「有沒有真的剝掉東西」推導 —— 有剝才叫重試。
    fn trace(&self, status: u16, stripped: Vec<String>, note: &str) -> TraceRecord {
        TraceRecord {
            app: self.app.to_string(),
            model_raw: self.model_raw.to_string(),
            in_fmt: self.in_fmt.as_str().to_string(),
            target_fmt: self.target_fmt.as_str().to_string(),
            trans_kind: self.kind.as_str().to_string(),
            upstream_status: status,
            latency_ms: self.started.elapsed().as_millis() as i64,
            retry_count: if stripped.is_empty() { 0 } else { 1 },
            stripped_fields: stripped,
            content_type: self.content_type.to_string(),
            note: note.to_string(),
            ..Default::default()
        }
    }

    fn connect_failed(&self, e: &reqwest::Error) -> Response {
        reject(
            self.ctx,
            self.started,
            self.app,
            self.model_raw,
            StatusCode::BAD_GATEWAY,
            format!("上游連接失敗：{e}"),
        )
    }
}

/// 送出請求；若上游回 400 且能從錯誤訊息解析出拒收欄位名，記住該欄位、
/// 剝離後重試一次。回傳最終要用的上游響應，或已備好的拒絕回應
/// （`Box<Response>`：`Response` 很大，直接當 Err 會讓 Result 過胖，
/// 見 clippy::result_large_err）。
///
/// 學到的欄位持久化於 SQLite 的 `provider_stripped_fields`：原實作只存在
/// 進程記憶體，網關每次停止／啟動即歸零，導致每個渠道的第一個請求都要
/// 重踩一次 400 再重試（見 §3 B2）。正常情況下 `strip_for_upstream` 已先
/// 套用這份記憶，所以不會走到這裡。
async fn send_with_strip_retry(
    rc: &RetryCtx<'_>,
    provider_id: i64,
    up: &Upstream<'_>,
    body: Vec<u8>,
) -> Result<reqwest::Response, Box<Response>> {
    let upstream = match up.send(body.clone()).await {
        Ok(r) => r,
        Err(e) => return Err(Box::new(rc.connect_failed(&e))),
    };
    if upstream.status() != StatusCode::BAD_REQUEST {
        return Ok(upstream);
    }
    // 400 且報拒收欄位 → 記住並剝離重發一次（New-API 系 unknown field；
    // OpenRouter 系 Unsupported parameter(s)；多個一次全剝離）；
    // 仍失敗或無法解析則透出上游原文，不再吞錯。
    let eb: Vec<u8> = upstream.bytes().await.unwrap_or_default().to_vec();
    // 客戶端訊息用截斷版；追蹤表存完整原文（§5.3 第 0 層）
    let upstream_text = upstream_err_text(&eb);
    let upstream_full = String::from_utf8_lossy(&eb).to_string();
    let mut retried: Option<reqwest::Response> = None;
    let mut applied: Vec<String> = vec![];
    let fields = parse_unknown_fields(&String::from_utf8_lossy(&eb));
    if !fields.is_empty() {
        let mut nb = body.clone();
        // 用短命連線寫記憶，而不是借用呼叫端的連線：
        // `rusqlite::Connection` 是 Send 但**不是 Sync**，所以 `&Connection`
        // 跨 await 會讓整個 future 變成 !Send，axum 的 Handler 就不成立。
        // 開新連線也與 `trace::log_to` 的既有做法一致。
        let mem = open_conn(&rc.ctx.db_path).ok();
        for field in &fields {
            if let Some(n) = strip_json_field(&nb, field) {
                match mem.as_ref().map(|c| trace::remember_stripped(c, provider_id, field)) {
                    Some(Err(e)) => eprintln!("gateway: 記錄拒收欄位失敗: {e}"),
                    None => eprintln!("gateway: 記錄拒收欄位失敗: 資料庫不可用"),
                    Some(Ok(())) => {}
                }
                nb = n;
                applied.push(field.clone());
            }
        }
        drop(mem);
        if !applied.is_empty() {
            // 剝離事件留痕：這是回答「這個 400 到底剝了什麼」的唯一來源。
            // 記的是**原始** body，不是剝完的 nb（否則看不出剝了什麼）。
            let rec = rc
                .trace(400, applied.clone(), "上游 400 拒收欄位，已剝離並重試")
                .with_body(&body)
                .with_upstream_error(&upstream_full);
            trace::log_to(&rc.ctx.db_path, &rec);
            match up.send(nb).await {
                Ok(r) => retried = Some(r),
                Err(e) => return Err(Box::new(rc.connect_failed(&e))),
            }
        }
    }
    match retried {
        Some(r) if !r.status().is_client_error() => Ok(r),
        Some(r) => {
            let st = r.status();
            let eb2 = r.bytes().await.unwrap_or_default().to_vec();
            let eb2_full = String::from_utf8_lossy(&eb2).to_string();
            // 剝離後仍失敗 → 這是真正未解決的 400，完整留痕
            let rec = rc
                .trace(st.as_u16(), applied.clone(), "剝離後重試仍失敗（未解決）")
                .with_body(&body)
                .with_upstream_error(&eb2_full);
            trace::log_to(&rc.ctx.db_path, &rec);
            Err(Box::new(reject(
                rc.ctx,
                rc.started,
                rc.app,
                rc.model_raw,
                st,
                upstream_err_text(&eb2),
            )))
        }
        None => {
            // 無法從錯誤訊息解析出欄位名 → 相容策略失效，必須留痕才能改進
            let note = if applied.is_empty() {
                "上游 400 且無法解析出拒收欄位名（相容策略失效）"
            } else {
                "上游 400（剝離未命中任何欄位）"
            };
            let rec = rc
                .trace(400, applied.clone(), note)
                .with_body(&body)
                .with_upstream_error(&upstream_full);
            trace::log_to(&rc.ctx.db_path, &rec);
            Err(Box::new(reject(
                rc.ctx,
                rc.started,
                rc.app,
                rc.model_raw,
                StatusCode::BAD_REQUEST,
                upstream_text,
            )))
        }
    }
}

/// 回應收尾階段所需的請求脈絡。
struct FinishCtx<'a> {
    ctx: &'a ProxyCtx,
    started: &'a Instant,
    app: &'a str,
    model_raw: &'a str,
    /// 送往上游的實際模型名（翻譯或直通改寫時）；未改寫則為空字串。
    translated_model: &'a str,
    body_json: &'a Option<serde_json::Value>,
    /// 本地 Key 的 id；直連上游模式為 None。
    key_id: Option<i64>,
    provider_id: i64,
    translated: bool,
    kind: TransKind,
}

/// 收尾：把上游回應轉回客戶端要的形狀、寫用量日誌、回傳響應。
///
/// - **串流**：依 `kind`／`translated` 選一個轉送分支，各自 spawn 一個 task
///   邊收邊轉邊送，流結束時記帳（見 `relay_sse` / `StreamLog`）。
/// - **非流式**：整包讀完再轉換形狀。只有成功狀態才轉 —— 錯誤回包必須原樣
///   透出，因為客戶端是靠狀態碼與 `error` 欄位讀訊息的。
///
/// 注意：每個 spawn 分支都必須在 `async move` **之外**先把要用的字串
/// 複製成 owned（`f.app.to_string()` 等）。直接把 `f` 搬進 task 不行
/// ——`f` 是借用，`tokio::spawn` 要求 `'static`。
async fn finish_response(upstream: reqwest::Response, f: &FinishCtx<'_>) -> Response {
    let status = upstream.status();
    let latency_ms = f.started.elapsed().as_millis() as i64;
    let is_sse = upstream
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("text/event-stream");

    // 轉發上游標頭（略過 hop-by-hop 與 content-encoding —— 後者是因為我們
    // 已經要了 identity，且 body 可能被改寫過）
    let mut builder = Response::builder().status(status);
    for (k, v) in upstream.headers().iter() {
        let name = k.as_str();
        if is_hop_header(name) || name == "content-encoding" {
            continue;
        }
        builder = builder.header(name, v.clone());
    }

    let key_id = f.key_id;
    let provider_id = f.provider_id;
    let db_path = f.ctx.db_path.clone();
    // 記帳模型：送往上游的實際模型（翻譯或直通改寫時），否則用客戶端模型
    //（上游回顯優先，見下方 usage.model）
    let log_fallback = if !f.translated_model.is_empty() {
        f.translated_model.to_string()
    } else {
        f.model_raw.to_string()
    };

    // Codex 自訂（freeform）工具名：上游端已被 function 化送出
    //（見 translate::responses_to_openai），回程把呼叫還原成 custom_tool_call。
    let custom_tools: Vec<String> = if f.kind == TransKind::ResponsesToChat {
        f.body_json
            .as_ref()
            .map(crate::translate::custom_tool_names)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if is_sse {
        // 流式：邊轉邊解析，結束寫日誌（翻譯分支按 kind 轉換事件形狀）
        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<axum::body::Bytes, axum::Error>>(64);
        if f.kind == TransKind::ResponsesToChat {
            // Responses 流式：chat chunk → Responses 事件；流結束補發 completed/failed
            let appv = f.app.to_string();
            let echo = f.model_raw.to_string();
            let logm = log_fallback.clone();
            let customs = custom_tools.clone();
            tokio::spawn(async move {
                let on_line = |s: &mut ResponsesRelay, t: &str| {
                    responses_line_events(t, &mut s.tstate, &echo, &mut s.acc, &customs)
                };
                let on_finish = |s: &mut ResponsesRelay| {
                    crate::translate::responses_stream_finish(
                        &mut s.tstate,
                        &echo,
                        &s.acc,
                        &customs,
                    )
                };
                let st =
                    relay_sse(upstream, &tx, ResponsesRelay::default(), on_line, on_finish)
                        .await;
                StreamLog::new(
                    &db_path,
                    &appv,
                    provider_id,
                    key_id,
                    latency_ms,
                    status.as_u16(),
                )
                .write(&logm, &st.acc);
            });
            return sse_response(builder, rx);
        }
        if f.translated {
            let appv = f.app.to_string();
            let echo = f.model_raw.to_string();
            let logm = log_fallback.clone();
            tokio::spawn(async move {
                let on_line = |s: &mut AnthropicRelay, t: &str| {
                    let Some(d) = t.strip_prefix("data:") else {
                        return (vec![], false);
                    };
                    let d = d.trim_start();
                    if d.is_empty() || d == "[DONE]" {
                        return (vec![], false);
                    }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(d) else {
                        return (vec![], false);
                    };
                    // 上游錯誤 → Anthropic error 事件並收流
                    if let Some(err) = v.get("error") {
                        let err = if err.is_null() { v.clone() } else { err.clone() };
                        return (vec![crate::translate::anthropic_error_line(&err)], true);
                    }
                    (
                        crate::translate::openai_chunk_to_anthropic(
                            &mut s.tstate,
                            &v,
                            &echo,
                            &mut s.acc,
                        ),
                        false,
                    )
                };
                let on_finish = |_s: &mut AnthropicRelay| Vec::new();
                let st =
                    relay_sse(upstream, &tx, AnthropicRelay::default(), on_line, on_finish)
                        .await;
                StreamLog::new(
                    &db_path,
                    &appv,
                    provider_id,
                    key_id,
                    latency_ms,
                    status.as_u16(),
                )
                .write(&logm, &st.acc);
            });
            return sse_response(builder, rx);
        }
        let appv = f.app.to_string();
        let mr = f.model_raw.to_string();
        tokio::spawn(async move {
            let mut acc = SseAcc::new();
            let mut stream = upstream.bytes_stream();
            use tokio_stream::StreamExt;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        acc.feed(&chunk);
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(axum::Error::new(e))).await;
                        break;
                    }
                }
            }
            acc.finish();
            let u = acc.usage;
            let model = u.model.clone().unwrap_or(mr);
            StreamLog::new(
                &db_path,
                &appv,
                provider_id,
                key_id,
                latency_ms,
                status.as_u16(),
            )
            .write(&model, &u);
        });
        return sse_response(builder, rx);
    }

    // 非流式：整包解析後轉發（翻譯分支轉成 Anthropic message）
    let resp_bytes = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return reject(
                f.ctx,
                f.started,
                f.app,
                f.model_raw,
                StatusCode::BAD_GATEWAY,
                format!("讀取上游響應失敗：{e}"),
            )
        }
    };
    let mut usage = SseUsage::default();
    let mut out_bytes = resp_bytes.to_vec();
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&resp_bytes) {
        extract_usage(&v, &mut usage);
        // 僅成功狀態才轉換形狀；錯誤回包原樣透出（客戶端按狀態碼讀錯誤）
        if f.translated && status.is_success() {
            if f.kind == TransKind::ResponsesToChat {
                let r = crate::translate::openai_to_responses(&v, f.model_raw, &custom_tools);
                out_bytes = serde_json::to_vec(&r).unwrap_or(out_bytes);
            } else {
                let a = crate::translate::openai_to_anthropic(&v, f.model_raw);
                out_bytes = serde_json::to_vec(&a).unwrap_or(out_bytes);
            }
        }
    }
    let model = usage.model.clone().unwrap_or(log_fallback);
    let norm = normalize_model(&model);
    // 這裡開短命連線（而非借用呼叫端的）：`&Connection` 不是 Send，
    // 會讓整個 future 變成 !Send（詳見 send_with_strip_retry 的註解）。
    if let Ok(conn) = open_conn(&db_path) {
        let ts = now_ms();
        let rates = crate::usage::resolve_rates(&conn, Some(provider_id), &norm, ts);
        let cost = crate::usage::rate_cost(
            &rates,
            usage.in_tok,
            usage.out_tok,
            usage.cache_read,
            usage.cache_write,
        );
        let _ = insert_log(
            &conn,
            ts,
            key_id,
            f.app,
            Some(provider_id),
            &model,
            &norm,
            &usage,
            cost,
            latency_ms,
            status.as_u16() as i64,
            false,
        );
        if let Some(kid) = key_id {
            let _ = keys::add_used(&conn, kid, usage.total());
        }
    }
    builder.body(Body::from(out_bytes)).unwrap_or_else(|_| {
        err_json(StatusCode::INTERNAL_SERVER_ERROR, "構造響應失敗".into())
    })
}

/// 前置階段的產物。
struct Prelude {
    authed: keys::AuthedKey,
    bytes: axum::body::Bytes,
    /// 已開好的連線。**擁有**它（而非借用）是刻意的：`Connection` 是 `Send`，
    /// 所以可以安全地跨 `await` 持有；`&Connection` 不是（見 `RetryCtx`）。
    conn: rusqlite::Connection,
}

/// 前置階段：解析 Bearer → 本地 Key（401 則回退直連上游）→ 限流 → 讀請求體。
///
/// 抽出來的理由是這一段有 **5 個拒絕出口**，每個都要「記被拒日誌 + 回錯誤
/// JSON」；混在主管線裡會讓真正的請求處理流程難以看清。
///
/// `Err` 用 `Box<Response>`（`Response` 有 128 bytes，直接當 Err 會觸發
/// clippy::result_large_err，與 `send_with_strip_retry` 一致）。
async fn prelude(
    ctx: &ProxyCtx,
    started: &Instant,
    app: &str,
    headers: &HeaderMap,
    body: Body,
) -> Result<Prelude, Box<Response>> {
    // ---- 鑑權
    let Some(secret) = bearer(headers) else {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            "",
            StatusCode::UNAUTHORIZED,
            "缺少 Authorization：請填本地 sk-local-… 或該渠道的上游 Key",
        )));
    };
    let conn = match open_conn(&ctx.db_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(reject(
                ctx,
                started,
                app,
                "",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("資料庫不可用：{e}"),
            )))
        }
    };
    // 本地 Key 優先；401 再回退直連上游（Bearer 即渠道自身 api_key，不經本地 Key 轉發）
    let key_err = |(code, msg): (u16, String)| {
        reject(
            ctx,
            started,
            app,
            "",
            StatusCode::from_u16(code).unwrap_or(StatusCode::UNAUTHORIZED),
            msg,
        )
    };
    let authed = match keys::auth_key(&conn, &secret) {
        Ok(k) => k,
        Err((401, _)) => match keys::auth_direct(&conn, &secret) {
            Ok(k) => k,
            Err(e) => return Err(Box::new(key_err(e))),
        },
        Err(e) => return Err(Box::new(key_err(e))),
    };
    if !ctx.rate.check(authed.id, authed.rate_limit_qpm) {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            "",
            StatusCode::TOO_MANY_REQUESTS,
            format!("該 Key 限流中（{}次/分鐘）", authed.rate_limit_qpm),
        )));
    }

    // 讀請求體（32MB 上限）
    const LIMIT: usize = 32 * 1024 * 1024;
    let bytes = match axum::body::to_bytes(body, LIMIT).await {
        Ok(b) => b,
        Err(_) => {
            return Err(Box::new(reject(
                ctx,
                started,
                app,
                "",
                StatusCode::PAYLOAD_TOO_LARGE,
                "請求體超過 32MB",
            )))
        }
    };
    Ok(Prelude {
        authed,
        bytes,
        conn,
    })
}

/// 模型白名單檢查 + 來源解析。
///
/// 兩件事放在一起，是因為它們共用同一組拒絕出口（模型名／來源名），
/// 且都可能在「模型不在清單內」時報錯。
///
/// - 白名單：Key 可限制可調用的模型與來源（空集合＝不限制）。
/// - 來源解析：先比對 Key 綁定來源的模型表；未命中再跨來源按模型路由
///   （其他啟用來源登記的同名模型同樣可走，Key 的額度／限流照常記在 Key 上）；
///   兩者都沒有才 400。
/// - **直連上游模式不做跨來源路由**：僅允許該渠道登記的模型。
///
/// 成功且跨來源路由命中時，會就地改寫 `authed` 的 `provider_*` 欄位。
///
/// 錯誤訊息刻意寫得囉唆：上游對「模型不存在」只回 cryptic 的 404/400，
/// 這裡直接指明「綁定來源是誰、該往哪個方向換 Key」，讓使用者能自己修。
///
/// 這裡可以安全地借用 `&Connection`：本函式是**同步**的，沒有 await 點，
/// 因此不受「`&Connection` 不是 `Send`」的限制（見 `RetryCtx`）。
fn resolve_model(
    ctx: &ProxyCtx,
    started: &Instant,
    app: &str,
    conn: &rusqlite::Connection,
    model_raw: &str,
    authed: &mut keys::AuthedKey,
) -> Result<(), Box<Response>> {
    if !authed.allowed_models.is_empty()
        && !model_raw.is_empty()
        && !authed.allowed_models.iter().any(|m| m == model_raw)
    {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許調用模型 {model_raw}"),
        )));
    }
    if !authed.allowed_apps.is_empty()
        && app != "unknown"
        && !authed.allowed_apps.iter().any(|a| a == app)
    {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許來源 {app}"),
        )));
    }

    if model_raw.is_empty() {
        return Ok(());
    }
    let list = crate::models::models_list(conn, authed.provider_id).unwrap_or_default();
    let usable: Vec<_> = list.into_iter().filter(|m| m.enabled).collect();
    let want = model_raw.to_lowercase();
    let bound_hit = usable.iter().any(|m| {
        m.display_name.to_lowercase() == want || m.actual_model.to_lowercase() == want
    });
    if usable.is_empty() || bound_hit {
        return Ok(());
    }
    if authed.direct {
        let pname = provider_name(conn, authed.provider_id);
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::BAD_REQUEST,
            format!(
                "直連模式僅允許來源「{pname}」登記的模型，{model_raw} 不在其清單內，請求不會轉發"
            ),
        )));
    }
    match resolve_model_provider(conn, model_raw, authed.provider_id) {
        Some(p) => {
            // 跨來源路由命中：改走登記該模型的來源
            authed.provider_id = p.id;
            authed.provider_base_url = p.base_url;
            authed.provider_api_key = p.api_key;
            authed.provider_auth_scheme = p.scheme;
            authed.provider_api_format = p.format;
            Ok(())
        }
        None => {
            let pname = provider_name(conn, authed.provider_id);
            Err(Box::new(reject(
                ctx,
                started,
                app,
                model_raw,
                StatusCode::BAD_REQUEST,
                format!(
                    "模型 {model_raw} 不在該 Key 綁定的來源「{pname}」模型清單內，請求不會轉發；請換用綁定正確來源的 Key；若該來源實際支援此模型，請先在來源的使用模型中新增對應映射"
                ),
            )))
        }
    }
}

/// `prepare_request` 的輸入（欄位較多，故用參數物件而非長參數列）。
struct PrepareInput<'a> {
    ctx: &'a ProxyCtx,
    started: &'a Instant,
    app: &'a str,
    conn: &'a rusqlite::Connection,
    provider_id: i64,
    /// 渠道協議（`providers.api_format`），用於矩陣判定。
    api_format: &'a str,
    model_raw: &'a str,
    content_type: &'a str,
    path_hint: &'a str,
    raw: &'a [u8],
    body_json: &'a Option<serde_json::Value>,
}

/// 請求準備的產物。
struct Prepared {
    in_fmt: InFmt,
    target_fmt: TargetFmt,
    kind: TransKind,
    translated: bool,
    /// 已轉譯、已剝離，可以直接送上游的位元組。
    bytes: Vec<u8>,
    /// 送往上游的實際模型名（未改寫則空字串）。
    model: String,
}

/// 格式矩陣判定 → 請求體轉譯 → 剝離上游拒收結構。
///
/// - 矩陣由 `resolve_trans_kind` 全表窮舉（編譯器保證，見 §3 B5/B6）；
///   不支援的組合先留一筆 warn 追蹤（`trans_kind: "unsupported"`）再回 400。
/// - 翻譯路上請求體不是 JSON 時，留下**原始位元組的 hex** 再回 400 ——
///   這是 §5.2 那個懸而未決的「body 解析失敗 400」的定案依據。
/// - 最後套用 `strip_for_upstream`（渠道記憶欄位、encrypted_content、
///   不支援的 tool types、直通歷史配對修復）。
///
/// 本函式是同步的，所以可以安全借用 `&Connection`。
fn prepare_request(input: PrepareInput<'_>) -> Result<Prepared, Box<Response>> {
    let in_fmt = InFmt::from_path(input.path_hint);
    let target_fmt = TargetFmt::from_db(input.api_format);
    let kind = match resolve_trans_kind(in_fmt, target_fmt) {
        Ok(k) => k,
        Err(msg) => {
            trace::log_to(
                &input.ctx.db_path,
                &TraceRecord {
                    app: input.app.to_string(),
                    model_raw: input.model_raw.to_string(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: "unsupported".to_string(),
                    upstream_status: 400,
                    latency_ms: input.started.elapsed().as_millis() as i64,
                    note: msg.to_string(),
                    ..Default::default()
                }
                .warn(),
            );
            return Err(Box::new(reject(
                input.ctx,
                input.started,
                input.app,
                input.model_raw,
                StatusCode::BAD_REQUEST,
                msg,
            )));
        }
    };
    let translated = kind != TransKind::None;
    // 流式請求強制索取用量：僅 OpenAI chat（含翻譯後）需要顯式 stream_options；
    // Responses / Anthropic / Gemini 加此欄位會被上游 400。
    let want_usage_opt = should_inject_usage(translated, input.path_hint);
    let (bytes, model) = match translate_forward_body(
        input.conn,
        input.provider_id,
        input.raw,
        input.body_json,
        input.model_raw,
        TransSpec { kind, translated },
        want_usage_opt,
    ) {
        BodyPrep::Ready(fb) => (fb.bytes, fb.model),
        BodyPrep::Unparsable => {
            // ── §5.2 的關鍵修復 ──
            // 本專案的「body 解析失敗 400」一直無法判定根因：docs/evidence/
            // 的三份樣本裡，成功案例的 bytes_len 是原始長度，兩個失敗案例卻是
            // 「去引號後」的長度，無法區分「真解析失敗」與「debug 儀器弄壞 body」。
            // 這裡把**原始位元組前綴的 hex** 落庫，下次失敗即可直接定案。
            trace::log_to(
                &input.ctx.db_path,
                &TraceRecord {
                    app: input.app.to_string(),
                    model_raw: input.model_raw.to_string(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: kind.as_str().to_string(),
                    upstream_status: 400,
                    latency_ms: input.started.elapsed().as_millis() as i64,
                    content_type: input.content_type.to_string(),
                    note: format!(
                        "請求體不是合法 JSON，無法翻譯（原始 {} bytes，已記錄 hex）",
                        input.raw.len()
                    ),
                    ..Default::default()
                }
                .with_body_hex(input.raw)
                .warn(),
            );
            return Err(Box::new(reject(
                input.ctx,
                input.started,
                input.app,
                input.model_raw,
                StatusCode::BAD_REQUEST,
                format!("{} 請求體不是 JSON，無法轉換為上游格式", in_fmt.label()),
            )));
        }
    };
    // 預先剝離該渠道已知拒收欄位與已知不支援結構（詳見 strip_for_upstream）。
    let bytes = strip_for_upstream(input.conn, input.provider_id, bytes, in_fmt, translated);
    Ok(Prepared {
        in_fmt,
        target_fmt,
        kind,
        translated,
        bytes,
        model,
    })
}

/// 一次請求的四個元資料，後續每個階段都要用。
struct ReqMeta {
    /// 解析後的請求體（不是合法 JSON 時為 `None`；錯誤留給 prepare_request 回報）
    body_json: Option<serde_json::Value>,
    /// 請求路徑，用於判定入站格式（`InFmt::from_path`）
    path_hint: String,
    content_type: String,
    /// 模型名。Gemini 原生請求體沒有 `model` 欄位，從 URL 路徑回填。
    model_raw: String,
}

/// 從請求標頭與本體取出後續階段要用的四個值。
///
/// 模型名的來源順序是刻意的：**請求體優先，其次 URL 路徑**。Gemini 原生
/// 端點把模型放在路徑（`/v1beta/models/gemini-2.5-pro:generateContent`），
/// 只有路徑回填才能讓白名單與記帳拿到正確的模型名。
fn request_meta(parts: &axum::http::request::Parts, bytes: &[u8]) -> ReqMeta {
    let body_json = parse_body_json(bytes);
    let path_hint = parts.uri.path().to_string();
    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let model_raw = body_json
        .as_ref()
        .and_then(|v| v.get("model"))
        .and_then(|m| m.as_str())
        .map(|s| s.to_string())
        .or_else(|| model_from_path(&path_hint))
        .unwrap_or_default();
    ReqMeta {
        body_json,
        path_hint,
        content_type,
        model_raw,
    }
}

/// 組裝上游連線物件（URL + 轉發標頭 + 方法 + client）。
///
/// 只是把 `build_upstream_target` 的結果（URL 與標頭）與 `client`／`method`
/// 綁成 `Upstream`，讓呼叫端少一段純粹的欄位搬運。
fn upstream_for<'a>(
    ctx: &'a ProxyCtx,
    parts: &axum::http::request::Parts,
    authed: &keys::AuthedKey,
    translated: bool,
    app: &str,
) -> Upstream<'a> {
    let (url, headers) = build_upstream_target(parts, authed, translated, app);
    Upstream {
        client: &ctx.client,
        method: parts.method.clone(),
        url,
        headers,
    }
}

async fn proxy_handler(State(ctx): State<ProxyCtx>, req: axum::http::Request<Body>) -> Response {
    let started = Instant::now();
    let (parts, body) = req.into_parts();
    // 來源判定只做一次（原本在每個拒絕分支各算一次，共 6 次）
    let app = infer_app(&parts.headers);

    // ---- 鑑權 → 限流 → 讀請求體（見 prelude）
    let Prelude {
        mut authed,
        bytes,
        conn,
    } = match prelude(&ctx, &started, &app, &parts.headers, body).await {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    // ---- 請求元資料（模型名／路徑／內容類型；見 request_meta）
    let ReqMeta {
        body_json,
        path_hint,
        content_type,
        model_raw,
    } = request_meta(&parts, &bytes);
    // ---- 模型白名單 + 來源解析（見 resolve_model）
    if let Err(resp) = resolve_model(&ctx, &started, &app, &conn, &model_raw, &mut authed) {
        return *resp;
    }

    // ---- 格式矩陣 + 請求體轉譯與剝離（見 prepare_request）
    let prep = match prepare_request(PrepareInput {
        ctx: &ctx,
        started: &started,
        app: &app,
        conn: &conn,
        provider_id: authed.provider_id,
        api_format: &authed.provider_api_format,
        model_raw: &model_raw,
        content_type: &content_type,
        path_hint: &path_hint,
        raw: &bytes,
        body_json: &body_json,
    }) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };
    let Prepared {
        in_fmt,
        target_fmt,
        kind,
        translated,
        bytes: body_bytes,
        model: translated_model,
    } = prep;

    // ---- 組裝上游請求（翻譯時固定打 /chat/completions）
    let up = upstream_for(&ctx, &parts, &authed, translated, &app);

    let rc = RetryCtx {
        ctx: &ctx,
        started: &started,
        app: &app,
        model_raw: &model_raw,
        content_type: &content_type,
        in_fmt,
        target_fmt,
        kind,
    };
    let upstream = match send_with_strip_retry(&rc, authed.provider_id, &up, body_bytes).await {
        Ok(r) => r,
        Err(resp) => return *resp,
    };
    let fc = FinishCtx {
        ctx: &ctx,
        started: &started,
        app: &app,
        model_raw: &model_raw,
        translated_model: &translated_model,
        body_json: &body_json,
        key_id: if authed.direct { None } else { Some(authed.id) },
        provider_id: authed.provider_id,
        translated,
        kind,
    };
    finish_response(upstream, &fc).await
}

pub async fn serve(db_path: PathBuf, listener: TcpListener) -> Result<(), String> {
    // 追蹤表上限：啟動時裁剪一次，避免長期運行無限成長。
    if let Ok(conn) = open_conn(&db_path) {
        if let Err(e) = trace::prune_traces(&conn, TRACE_KEEP) {
            eprintln!("gateway: 裁剪 proxy_trace 失敗: {e}");
        }
    }
    let ctx = ProxyCtx {
        db_path,
        client: Client::builder()
            // 連線階段上限：上游 TCP 可達但握手卡住時，不該等滿整體 timeout（§3 B9）
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            // 整體請求上限（含串流讀取）
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| e.to_string())?,
        rate: RateLimiter::default(),
    };
    let app = axum::Router::new()
        .fallback(proxy_handler)
        .with_state(ctx);
    axum::serve(listener, app).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 單測 ---

#[cfg(test)]
mod tests;
