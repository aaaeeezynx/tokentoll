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

/// 入站→渠道翻譯類型（None = 原樣透傳）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransKind {
    None,
    AnthropicToChat,
    ResponsesToChat,
}

impl TransKind {
    fn as_str(self) -> &'static str {
        match self {
            TransKind::None => "none",
            TransKind::AnthropicToChat => "anthropic_to_chat",
            TransKind::ResponsesToChat => "responses_to_chat",
        }
    }
}

/// 入站格式（由請求路徑判定）。
///
/// 改用 enum（原為 `&str`）是為了讓格式矩陣能被**編譯器**窮舉檢查：
/// `match (InFmt, TargetFmt)` 若少列一格就編譯失敗，從根本上消除
/// 「正確性依賴分支順序」的脆弱性（見 docs/REFACTORING-PLAN.md §3 B5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InFmt {
    Anthropic,
    Responses,
    OpenAi,
    Gemini,
    /// 未知／中性路徑（如 `/v1/models`）：一律透傳，不誤傷。
    Unknown,
}

impl InFmt {
    fn from_path(path: &str) -> Self {
        if path.contains(":generateContent")
            || path.contains(":streamGenerateContent")
            || path.contains(":embedContent")
            || path.contains("v1beta/models")
        {
            InFmt::Gemini
        } else if path.ends_with("/messages") {
            InFmt::Anthropic
        } else if path.ends_with("/responses") {
            InFmt::Responses
        } else if path.contains("/chat/completions")
            || path.contains("/embeddings")
            || path.ends_with("/models")
        {
            InFmt::OpenAi
        } else {
            InFmt::Unknown
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            InFmt::Anthropic => "anthropic",
            InFmt::Responses => "responses",
            InFmt::OpenAi => "openai",
            InFmt::Gemini => "gemini",
            InFmt::Unknown => "unknown",
        }
    }

    /// 給使用者看的格式名（錯誤訊息用）。
    fn label(self) -> &'static str {
        match self {
            InFmt::Anthropic => "Anthropic",
            InFmt::Responses => "Responses",
            InFmt::OpenAi => "OpenAI Chat",
            InFmt::Gemini => "Gemini",
            InFmt::Unknown => "入站",
        }
    }
}

/// 渠道協議（`providers.api_format`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetFmt {
    OpenAiChat,
    OpenAiResponses,
    /// 混合渠道：同時支援 chat 與 responses 端點（見 `db.rs` 的 zen 種子）。
    Mixed,
    Anthropic,
    Gemini,
    /// `api_format` 不在已知值內（拼錯或未來新增）。**不猜測協議**。
    Unknown,
}

impl TargetFmt {
    fn from_db(s: &str) -> Self {
        match s {
            "openai-chat" => TargetFmt::OpenAiChat,
            "openai-responses" => TargetFmt::OpenAiResponses,
            "mixed" => TargetFmt::Mixed,
            "anthropic" => TargetFmt::Anthropic,
            "gemini" => TargetFmt::Gemini,
            _ => TargetFmt::Unknown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            TargetFmt::OpenAiChat => "openai-chat",
            TargetFmt::OpenAiResponses => "openai-responses",
            TargetFmt::Mixed => "mixed",
            TargetFmt::Anthropic => "anthropic",
            TargetFmt::Gemini => "gemini",
            TargetFmt::Unknown => "unknown",
        }
    }
}

// 400 訊息（使用者直接看到，故集中管理）
const E_ANTHROPIC_UNSUPPORTED: &str =
    "該渠道不接受 Anthropic 請求：請換 Anthropic 官方或 OpenAI 兼容渠道";
const E_REVERSE_UNSUPPORTED: &str =
    "反向轉換（OpenAI → Anthropic）尚未實現，請換 OpenAI 兼容渠道";
const E_GEMINI_IN_ONLY: &str = "Gemini 原生請求只能走 Gemini 渠道";
const E_GEMINI_OUT_ONLY: &str = "Gemini 渠道只接受 Gemini 原生請求";
/// B6：chat 請求打到 responses 端點，原實作靜默直通而必然失敗，改為明確報錯。
const E_CHAT_TO_RESPONSES: &str =
    "此渠道為 Responses 協議，不接受 OpenAI Chat 請求：請改用 Chat 兼容渠道，或改用 Codex 等 Responses 客戶端";

/// 入站格式 × 渠道協議 → 翻譯類型（`Err` = 該組合不支援，字串即 400 訊息）。
///
/// **30 格全列舉，無萬用字元** —— 未來新增任何 `InFmt` / `TargetFmt` 變體，
/// 編譯器都會強制在此補上對應行為，不會靜默落到透傳。
/// 既有行為已逐格核對與原實作等價（僅 B6 一格由「靜默直通」改為「明確報錯」）。
fn resolve_trans_kind(in_fmt: InFmt, target: TargetFmt) -> Result<TransKind, &'static str> {
    use InFmt::*;
    use TargetFmt as T;
    match (in_fmt, target) {
        // ── Anthropic 入站 ──
        (Anthropic, T::Anthropic) => Ok(TransKind::None),
        (Anthropic, T::OpenAiChat) | (Anthropic, T::Mixed) => Ok(TransKind::AnthropicToChat),
        (Anthropic, T::OpenAiResponses)
        | (Anthropic, T::Gemini)
        | (Anthropic, T::Unknown) => Err(E_ANTHROPIC_UNSUPPORTED),

        // ── Responses 入站（Codex）──
        (Responses, T::OpenAiChat) | (Responses, T::Mixed) => Ok(TransKind::ResponsesToChat),
        (Responses, T::OpenAiResponses) => Ok(TransKind::None),
        (Responses, T::Anthropic) => Err(E_REVERSE_UNSUPPORTED),
        (Responses, T::Gemini) => Err(E_GEMINI_OUT_ONLY),
        (Responses, T::Unknown) => Ok(TransKind::None),

        // ── OpenAI Chat 入站 ──
        (OpenAi, T::OpenAiChat) | (OpenAi, T::Mixed) => Ok(TransKind::None),
        (OpenAi, T::OpenAiResponses) => Err(E_CHAT_TO_RESPONSES),
        (OpenAi, T::Anthropic) => Err(E_REVERSE_UNSUPPORTED),
        (OpenAi, T::Gemini) => Err(E_GEMINI_OUT_ONLY),
        (OpenAi, T::Unknown) => Ok(TransKind::None),

        // ── Gemini 原生入站 ──
        (Gemini, T::Gemini) => Ok(TransKind::None),
        (Gemini, T::OpenAiChat)
        | (Gemini, T::OpenAiResponses)
        | (Gemini, T::Mixed)
        | (Gemini, T::Anthropic)
        | (Gemini, T::Unknown) => Err(E_GEMINI_IN_ONLY),

        // ── 未知入站格式（中性路徑）──
        (Unknown, T::Gemini) => Err(E_GEMINI_OUT_ONLY),
        (Unknown, T::OpenAiChat)
        | (Unknown, T::OpenAiResponses)
        | (Unknown, T::Mixed)
        | (Unknown, T::Anthropic)
        | (Unknown, T::Unknown) => Ok(TransKind::None),
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

    // 模型名（用於白名單 + 日誌；Gemini 原生請求體無 model，從 URL 回填）
    let body_json: Option<serde_json::Value> = parse_body_json(&bytes);
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
    if !authed.allowed_models.is_empty()
        && !model_raw.is_empty()
        && !authed.allowed_models.iter().any(|m| m == &model_raw)
    {
        return reject(
            &ctx,
            &started,
            &app,
            &model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許調用模型 {model_raw}"),
        );
    }
    if !authed.allowed_apps.is_empty() && app != "unknown" && !authed.allowed_apps.contains(&app) {
        return reject(
            &ctx,
            &started,
            &app,
            &model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許來源 {app}"),
        );
    }

    // ---- 來源解析：先比對 Key 綁定來源的模型表；未命中再跨來源按模型路由
    //（其他啟用來源登記的同名模型同樣可走，Key 的額度/限流照常記在 Key 上）；
    // 兩者都沒有才 400（上游只回 cryptic 404/400，此處指明綁定來源與換 Key 方向）
    // 直連上游模式不做跨來源路由：僅允許該渠道登記的模型。
    if !model_raw.is_empty() {
        let list = crate::models::models_list(&conn, authed.provider_id)
            .unwrap_or_default();
        let usable: Vec<_> = list.into_iter().filter(|m| m.enabled).collect();
        let want = model_raw.to_lowercase();
        let bound_hit = usable.iter().any(|m| {
            m.display_name.to_lowercase() == want
                || m.actual_model.to_lowercase() == want
        });
        if !usable.is_empty() && !bound_hit {
            if authed.direct {
                let pname = provider_name(&conn, authed.provider_id);
                return reject(
                    &ctx,
                    &started,
                    &app,
                    &model_raw,
                    StatusCode::BAD_REQUEST,
                    format!(
                        "直連模式僅允許來源「{pname}」登記的模型，{model_raw} 不在其清單內，請求不會轉發"
                    ),
                );
            }
            match resolve_model_provider(&conn, &model_raw, authed.provider_id) {
                Some(p) => {
                    // 跨來源路由命中：改走登記該模型的來源
                    authed.provider_id = p.id;
                    authed.provider_base_url = p.base_url;
                    authed.provider_api_key = p.api_key;
                    authed.provider_auth_scheme = p.scheme;
                    authed.provider_api_format = p.format;
                }
                None => {
                    let pname = provider_name(&conn, authed.provider_id);
                    return reject(
                        &ctx,
                        &started,
                        &app,
                        &model_raw,
                        StatusCode::BAD_REQUEST,
                        format!(
                            "模型 {model_raw} 不在該 Key 綁定的來源「{pname}」模型清單內，請求不會轉發；請換用綁定正確來源的 Key；若該來源實際支援此模型，請先在來源的使用模型中新增對應映射"
                        ),
                    );
                }
            }
        }
    }

    // ---- 格式矩陣：入站格式 × 渠道格式（mixed 視為 OpenAI 兼容）
    // 全表列舉於 resolve_trans_kind，由編譯器保證窮舉（見 §3 B5/B6）。
    let in_fmt = InFmt::from_path(&path_hint);
    let target_fmt = TargetFmt::from_db(&authed.provider_api_format);
    let kind = match resolve_trans_kind(in_fmt, target_fmt) {
        Ok(k) => k,
        Err(msg) => {
            trace::log_to(
                &ctx.db_path,
                &TraceRecord {
                    app: app.clone(),
                    model_raw: model_raw.clone(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: "unsupported".to_string(),
                    upstream_status: 400,
                    latency_ms: started.elapsed().as_millis() as i64,
                    note: msg.to_string(),
                    ..Default::default()
                }
                .warn(),
            );
            return reject(
                &ctx,
                &started,
                &app,
                &model_raw,
                StatusCode::BAD_REQUEST,
                msg,
            );
        }
    };
    let translated = kind != TransKind::None;

    // 模型改寫 display→actual：翻譯分支在轉換時做，直通分支（OpenAI / Responses）
    // 在此改寫 body.model；無映射時 resolve_actual 原樣返回，行為不變。
    // 若不改寫，工具送顯示名（如 deepseek-v4-flash-0731）會被上游以 invalid model ID 拒收。
    let spec = TransSpec { kind, translated };
    // 流式請求強制索取用量：僅 OpenAI chat（含翻譯後）需要顯式 stream_options；
    // Responses / Anthropic / Gemini 加此欄位會被上游 400。
    let want_usage_opt = should_inject_usage(translated, &path_hint);
    let (mut body_bytes, translated_model) = match translate_forward_body(
        &conn,
        authed.provider_id,
        &bytes,
        &body_json,
        &model_raw,
        spec,
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
                &ctx.db_path,
                &TraceRecord {
                    app: app.clone(),
                    model_raw: model_raw.clone(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: kind.as_str().to_string(),
                    upstream_status: 400,
                    latency_ms: started.elapsed().as_millis() as i64,
                    content_type: content_type.clone(),
                    note: format!(
                        "請求體不是合法 JSON，無法翻譯（原始 {} bytes，已記錄 hex）",
                        bytes.len()
                    ),
                    ..Default::default()
                }
                .with_body_hex(&bytes)
                .warn(),
            );
            return reject(
                &ctx,
                &started,
                &app,
                &model_raw,
                StatusCode::BAD_REQUEST,
                format!("{} 請求體不是 JSON，無法轉換為上游格式", in_fmt.label()),
            );
        }
    };

    // ---- 組裝上游請求（翻譯時固定打 /chat/completions）
    let (url, fwd_headers) = build_upstream_target(&parts, &authed, translated, &app);
    let up = Upstream {
        client: &ctx.client,
        method: parts.method.clone(),
        url,
        headers: fwd_headers,
    };
    // 預先剝離該渠道已知拒收欄位與已知不支援結構（詳見 strip_for_upstream）。
    body_bytes = strip_for_upstream(&conn, authed.provider_id, body_bytes, in_fmt, translated);

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
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn stream_options_only_for_chat() {
        assert!(should_inject_usage(false, "/v1/chat/completions"));
        assert!(should_inject_usage(true, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1/responses"));
        assert!(!should_inject_usage(false, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1beta/models/x:generateContent"));
    }

    #[test]
    fn unknown_field_parse_and_strip() {
        assert_eq!(
            parse_unknown_fields(r#"json: unknown field "verbosity" Request id: 1"#),
            vec!["verbosity"]
        );
        // JSON 轉義形態（真實回包樣子）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"json: unknown field \"verbosity\" Request id: 1"}"#),
            vec!["verbosity"]
        );
        assert!(parse_unknown_fields("plain 400").is_empty());
        assert!(parse_unknown_fields("unknown field \"\"").is_empty());
        let body = br#"{"model":"m","verbosity":"low","stream":true}"#;
        let stripped = strip_json_field(body, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("verbosity").is_none());
        assert_eq!(v["model"], "m");
        assert!(strip_json_field(body, "nope").is_none());
        assert!(strip_json_field(b"not json", "verbosity").is_none());
        // Codex 式：verbosity 藏在 text 裡，清空後 text 一併移除
        let nested = br#"{"model":"m","text":{"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("text").is_none());
        let nested2 = br#"{"model":"m","text":{"format":{"type":"text"},"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested2, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v["text"]["format"]["type"] == "text");
    }

    #[test]
    fn openrouter_unsupported_parameter_parse() {
        // OpenRouter 真實口徑（2026-09-24 codex 實例：prompt_cache_key 被拒）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"Validation: Unsupported parameter(s): 'prompt_cache_key'","type":"Bad Request","code":400}"#),
            vec!["prompt_cache_key"]
        );
        // 多個一次全取出
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): 'a', 'b-c_d'"),
            vec!["a", "b-c_d"]
        );
        assert_eq!(
            parse_unknown_fields("Validation: Unsupported parameter(s): `prompt_cache_key`"),
            vec!["prompt_cache_key"]
        );
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): \"a\", 'b'"),
            vec!["a", "b"]
        );
        assert!(parse_unknown_fields("Unsupported parameter(s): none").is_empty());
        // 剝離後重發體：prompt_cache_key 移除，其餘不動
        let body = br#"{"model":"m","prompt_cache_key":"sess-1","messages":[]}"#;
        let stripped = strip_json_field(body, "prompt_cache_key").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("prompt_cache_key").is_none());
        assert_eq!(v["model"], "m");
    }

    #[test]
    fn join_upstream_dedups_v1() {
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/chat/completions"),
            "https://router.flatkey.ai/v1/chat/completions"
        );
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/messages"),
            "https://router.flatkey.ai/v1/messages"
        );
        assert_eq!(
            join_upstream("https://host", "/v1/models"),
            "https://host/v1/models"
        );
    }

    #[test]
    fn normalize_model_rules() {
        assert_eq!(normalize_model("anthropic/claude-opus-4-8"), "claude-opus-4-8");
        assert_eq!(normalize_model("GPT-5.2-codex@low"), "gpt-5.2-codex-low");
        // -v1 / 日期 / 渠道尾綴剝除（短 ID 兜底定價）
        assert_eq!(
            normalize_model("global.anthropic.claude-opus-4-8-v1:0"),
            "global.anthropic.claude-opus-4-8"
        );
        assert_eq!(
            normalize_model("deepseek-v4-flash-ga-260731"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            normalize_model("gpt-4o-mini-2024-07-18"),
            "gpt-4o-mini"
        );
        assert_eq!(
            normalize_model("moonshotai/kimi-k2-0905:exa"),
            "kimi-k2-0905"
        );
        // 語義編號不受影響
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_model("llama-3"), "llama-3");
        assert_eq!(normalize_model("kimi-k2-turbo"), "kimi-k2-turbo");
        assert_eq!(normalize_model("kimi-k2-0905"), "kimi-k2-0905");
        assert_eq!(normalize_model("  kimi-k2-0905[1m] "), "kimi-k2-0905");
    }

    #[test]
    fn extract_openai_chat_usage() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"model":"gpt-5","usage":{"prompt_tokens":10,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":3}}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 5, 3));
        assert_eq!(u.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn extract_anthropic_stream_usage() {
        let start: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_start","message":{"model":"claude-opus-4-8","usage":{"input_tokens":7,"cache_read_input_tokens":2}}}"#,
        )
        .unwrap();
        let delta: serde_json::Value = serde_json::from_str(
            r#"{"type":"content_block_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        // message_delta 形狀
        let delta2: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&start, &mut u);
        extract_usage(&delta, &mut u);
        extract_usage(&delta2, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (7, 4, 2));
    }

    #[test]
    fn sse_accumulates_across_chunks() {
        let mut acc = SseAcc::new();
        acc.feed(b"data: {\"model\":\"gpt-5\",\"choices\":[]}\n\ndata: ");
        acc.feed(b"{\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n");
        assert_eq!((acc.usage.in_tok, acc.usage.out_tok), (10, 5));
        assert_eq!(acc.usage.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn infer_app_rules() {
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static("codex-cli/1.0"));
        assert_eq!(infer_app(&h), "codex");
        h.insert("x-tg-app", HeaderValue::from_static("hermes"));
        assert_eq!(infer_app(&h), "hermes");
        h.insert("x-tg-app", HeaderValue::from_static("nope"));
        assert_eq!(infer_app(&h), "codex", "非法聲明回退到 UA");
        h.insert("x-tg-app", HeaderValue::from_static("cursor"));
        assert_eq!(infer_app(&h), "cursor");
        let mut h2 = HeaderMap::new();
        h2.insert(
            "user-agent",
            HeaderValue::from_static("antigravity/1.0"),
        );
        assert_eq!(infer_app(&h2), "antigravity");
    }

    #[test]
    fn extract_gemini_usage_metadata() {
        // 完整形狀：candidates + thoughts 併入 out
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":1,"totalTokenCount":64,"thoughtsTokenCount":58}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 59));
        // 被截斷回包：無 candidates，用 total-prompt 回退
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"totalTokenCount":18,"thoughtsTokenCount":13}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 13));
        // 快取欄位
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":2,"totalTokenCount":12,"cachedContentTokenCount":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 2, 4));
    }

    #[test]
    fn strip_key_and_model_from_path() {
        assert_eq!(
            strip_key_param(Some("alt=sse&key=SECRET&x=1")).as_deref(),
            Some("alt=sse&x=1")
        );
        assert_eq!(strip_key_param(Some("key=SECRET")), None);
        assert_eq!(strip_key_param(None), None);
        assert_eq!(
            model_from_path("/v1beta/models/gemini-3.6-flash:generateContent").as_deref(),
            Some("gemini-3.6-flash")
        );
        assert_eq!(model_from_path("/v1/chat/completions"), None);
    }

    /// Google 方向 e2e：斷言上游收到 x-goog-api-key 且無 Bearer，usageMetadata 正確記帳。
    #[tokio::test]
    async fn e2e_goog_key_forwarding() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("g.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('g','universal','gemini','PLACEHOLDER','up-key','goog-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
        }
        let fake = axum::Router::new().route(
            "/v1beta/models/gemini-3.6-flash:generateContent",
            axum::routing::post(
                |headers: HeaderMap, body: axum::Json<serde_json::Value>| async move {
                    assert_eq!(
                        headers.get("x-goog-api-key").and_then(|v| v.to_str().ok()),
                        Some("up-key")
                    );
                    assert!(headers.get("authorization").is_none(), "不應透出 Bearer");
                    assert!(body.get("model").is_none());
                    axum::Json(serde_json::json!({
                        "candidates": [{"content": {"parts": [{"text": "ok"}]}}],
                        "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 1, "totalTokenCount": 6}
                    }))
                    .into_response()
                },
            ),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='g'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='g'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "g".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        let r = http
            .post(format!(
                "http://127.0.0.1:{gw_port}/v1beta/models/gemini-3.6-flash:generateContent?key=CLIENTKEY"
            ))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"contents": [{"parts": [{"text": "hi"}]}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (model, itok, otok): (String, i64, i64) = conn
            .query_row(
                "SELECT model_raw, in_tok, out_tok FROM request_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // 模型從 URL 回填，用量按 usageMetadata 解析
        assert_eq!((model.as_str(), itok, otok), ("gemini-3.6-flash", 5, 1));
        gw.abort();
    }

    /// 端到端：假上游 + 真網關 + 真 DB。覆蓋非流式轉發/鑑權/日誌三件套。
    #[tokio::test]
    async fn e2e_forward_auth_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // 註：此處原宣告 'openai-responses'，但本測試走的是 chat 端點 ——
            // 它之所以一直綠燈，正是因為 B6 的靜默直通掩蓋了設定錯誤。
            // 改為 'openai-chat' 以符合本測試「chat 直通轉發」的意圖。
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('fake','codex','openai-chat','PLACEHOLDER','up-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
        }

        // 假上游：stream=true 回 SSE，否則回固定 usage JSON
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                if body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false) {
                    let sse = "data: {\"model\":\"gpt-5\",\"choices\":[]}\n\ndata: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                axum::Json(serde_json::json!({
                    "model": "gpt-5",
                    "usage": {"prompt_tokens": 11, "completion_tokens": 6}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='fake'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            // 注意：基址無 /v1 路徑，入站 /v1/chat/completions 原樣保留
        }

        // 建 Key
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='fake'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "e2e".into(),
                    provider_id: Some(pid),
                    quota_tokens: 1000,
                    rate_limit_qpm: 60,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };

        // 起網關
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));

        let http = Client::new();
        // 1) 無鑑權 → 401
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
        // 2) 正常轉發
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["usage"]["prompt_tokens"], 11);
        // 2c) B6 端到端回歸：把渠道改成 responses 協議後，同一個 chat 請求必須
        //     得到網關自己產生的明確 400，而不是被靜默轉發給上游後失敗。
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET api_format='openai-responses' WHERE name='fake'",
                [],
            )
            .unwrap();
        }
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "chat→responses 應明確拒絕（B6）");
        let err_text = r.text().await.unwrap();
        assert!(
            err_text.contains("Responses"),
            "錯誤訊息應說明協議不符: {err_text}"
        );
        // 訊息必須是可理解的中文說明，而非上游原始錯誤
        assert!(
            err_text.contains("Chat 兼容渠道") || err_text.contains("Responses 客戶端"),
            "應給出可行動的建議: {err_text}"
        );
        // 還原協議，避免影響後續流式步驟
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET api_format='openai-chat' WHERE name='fake'",
                [],
            )
            .unwrap();
        }
        // 2b) 流式轉發：SSE 原樣透出 + 結束落日誌
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .header("user-agent", "codex-e2e")
            .json(&serde_json::json!({"model": "gpt-5", "stream": true}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("prompt_tokens"), "SSE 應透出上游分塊");
        // 流式日誌在後臺任務寫庫，輪詢等待
        let (app2, i2, o2, s2): (String, i64, i64, i64) = {
            let mut last = ("".to_string(), -1i64, -1i64, -1i64);
            for _ in 0..20 {
                let conn = crate::db::open_and_ensure(&db_path).unwrap();
                let row: Option<(String, i64, i64, i64, i64)> = conn
                    .query_row(
                        "SELECT app, in_tok, out_tok, status, is_stream FROM request_logs ORDER BY id DESC LIMIT 1",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                    )
                    .ok();
                if let Some((a, i, o, st, is)) = row {
                    if is == 1 {
                        last = (a, i, o, st);
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            last
        };
        assert_eq!((app2.as_str(), i2, o2, s2), ("codex", 3, 2, 200));
        // 3) 非流式日誌落庫（取成功那一行；步驟 2c 的被拒 400 也會落一行，
        //    故必須以 status 區分 —— 這同時驗證「被拒請求也留痕」）
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let rejects: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM request_logs WHERE status=400",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rejects, 1, "B6 的 400 應留下一筆被拒日誌");
        let (app, itok, otok, status): (String, i64, i64, i64) = conn
            .query_row(
                "SELECT app, in_tok, out_tok, status FROM request_logs
                 WHERE is_stream=0 AND status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((app.as_str(), itok, otok, status), ("codex", 11, 6, 200));
        let used: i64 = conn
            .query_row("SELECT used_tokens FROM local_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(used, 17 + 5);
        // 4) 配額用盡 → 429
        conn.execute("UPDATE local_keys SET quota_tokens=22", []).unwrap();
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "gpt-5"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 429);

        gw.abort();
    }

    /// 跨來源防呆：Key 綁定來源登記了啟用模型時，未登記模型直接 400 指明來源，不轉發
    #[tokio::test]
    async fn e2e_unknown_model_blocked_with_hint() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('guard-src','codex','openai-chat','http://127.0.0.1:9','up-key','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='guard-src'", [], |r| r.get(0))
                .unwrap();
            crate::models::model_add(
                &conn,
                &crate::models::UsableModelInput {
                    provider_id: pid,
                    display_name: "real-model".into(),
                    actual_model: "real-model".into(),
                    context_window: None,
                    reasoning: "unset".into(),
                    ord: 0,
                    enabled: true,
                },
            )
            .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "e2e".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 60,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        // 未登記模型 → 400 且指明綁定來源
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "ghost-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        let body = r.text().await.unwrap();
        assert!(body.contains("guard-src"), "應指明綁定來源：{body}");
        assert!(body.contains("ghost-model"), "應指明請求模型：{body}");
        // 已登記模型 → 通過防呆（上游 127.0.0.1:9 不可達，轉發失敗非 400 即可）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "real-model"}))
            .send()
            .await
            .unwrap();
        assert_ne!(r.status(), 400);
        gw.abort();
    }

    /// Responses e2e：Codex 形狀 /v1/responses 進 → 翻譯成 chat 打上游 →
    /// 客戶端收到 Responses 形狀；流式含 created/delta/completed 事件；按實際模型記帳。
    #[tokio::test]
    async fn e2e_responses_to_chat() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("rs.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('nim','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'glm-test', 'actual-glm', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假上游：斷言收到 chat 形狀；回 chat 回包（流式回 SSE）
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "actual-glm");
                assert_eq!(body["messages"][0]["role"], "system");
                assert_eq!(body["messages"][0]["content"], "be nice");
                assert_eq!(body["messages"][1]["role"], "user");
                assert_eq!(body["messages"][1]["content"], "hi");
                assert_eq!(body["max_tokens"], 32);
                if body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false) {
                    let sse = "data: {\"id\":\"chatcmpl-rs1\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"he\"}}]}\n\ndata: {\"id\":\"chatcmpl-rs1\",\"choices\":[{\"delta\":{\"content\":\"llo\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"chatcmpl-rs1\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-rs1", "model": "actual-glm",
                    "choices": [{"message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 7, "completion_tokens": 3}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='nim'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "rs".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) 非流式：Responses 進 → Responses 出
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": "hi",
                "max_output_tokens": 32
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["object"], "response");
        assert_eq!(body["status"], "completed");
        assert_eq!(body["model"], "glm-test");
        assert_eq!(body["output"][0]["type"], "message");
        assert_eq!(body["output"][0]["content"][0]["text"], "hello");
        assert_eq!(body["usage"]["input_tokens"], 7);
        assert_eq!(body["usage"]["output_tokens"], 3);

        // 2) 流式：SSE 事件序列
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": "hi",
                "max_output_tokens": 32,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("event: response.created"));
        assert!(sse_body.contains("event: response.output_item.added"));
        assert!(sse_body.contains("response.output_text.delta"));
        assert!(sse_body.contains("\"delta\":\"he\""));
        assert!(sse_body.contains("event: response.output_item.done"));
        assert!(sse_body.contains("\"text\":\"hello\""));
        assert!(sse_body.contains("event: response.completed"));
        assert!(sse_body.contains("\"input_tokens\":7"));
        assert!(sse_body.contains("\"output_tokens\":3"));
        // 3) 記帳按實際送出模型
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let m: String = conn
            .query_row(
                "SELECT model_raw FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m, "actual-glm");
        gw.abort();
    }

    /// e2e：Codex 自訂（freeform）工具往返 + 上游錯誤改發 response.failed。
    #[tokio::test]
    async fn e2e_custom_tool_and_stream_error() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ct.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('nim','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'glm-test', 'actual-glm', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                // custom 工具必須 function 化送出
                let tools = body["tools"].as_array().unwrap();
                assert_eq!(tools.len(), 2, "exec_command + apply_patch");
                let ap = tools
                    .iter()
                    .find(|t| t["function"]["name"] == "apply_patch")
                    .expect("apply_patch 必須送出");
                assert_eq!(ap["type"], "function");
                assert_eq!(ap["function"]["parameters"]["required"][0], "input");
                // 歷史：custom_tool_call(_output) 已還原成 assistant tool_calls + tool
                let msgs = body["messages"].as_array().unwrap();
                let last_user = msgs
                    .iter()
                    .rev()
                    .find(|m| m["role"] == "user")
                    .unwrap();
                if last_user["content"] == "fail" {
                    let sse = "data: {\"error\":{\"message\":\"upstream boom\",\"type\":\"rate_limit_error\"}}\n\n";
                    return Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from(sse))
                        .unwrap();
                }
                let has_custom_call = msgs.iter().any(|m| {
                    m.get("tool_calls")
                        .and_then(|c| c.as_array())
                        .is_some_and(|a| a.iter().any(|t| t["function"]["name"] == "apply_patch"))
                });
                assert!(has_custom_call, "custom 歷史必須還原成 tool_calls");
                assert!(msgs.iter().any(|m| m["role"] == "tool"), "tool 結果必須保留");
                let args = serde_json::json!({"input": "*** Begin Patch\n*** End Patch"}).to_string();
                let c1 = serde_json::json!({"id":"chatcmpl-ct1","choices":[{"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_9","function":{"name":"apply_patch","arguments":args}}]}}]}).to_string();
                let sse = format!(
                    "data: {c1}\n\ndata: {{\"id\":\"chatcmpl-ct1\",\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: {{\"id\":\"chatcmpl-ct1\",\"choices\":[],\"usage\":{{\"prompt_tokens\":5,\"completion_tokens\":4}}}}\n\ndata: [DONE]\n\n"
                );
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from(sse))
                    .unwrap()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='nim'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='nim'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "ct".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        let tools = serde_json::json!([
            {"type": "function", "name": "exec_command", "description": "run",
             "parameters": {"type": "object"}},
            {"type": "custom", "name": "apply_patch", "description": "patch files",
             "format": {"type": "grammar", "syntax": "lark", "definition": "start: x"}}
        ]);

        // 1) 流式：custom 工具呼叫 → custom_tool_call 還原
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": [
                    {"type": "message", "role": "user", "content": "hi"},
                    {"type": "custom_tool_call", "call_id": "ctc1", "name": "apply_patch",
                     "input": "*** Begin Patch\n*** End Patch"},
                    {"type": "custom_tool_call_output", "call_id": "ctc1",
                     "output": "Success. Updated the following files:\nM a.ts"}
                ],
                "tools": tools,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("\"type\":\"custom_tool_call\""), "{sse_body}");
        assert!(sse_body.contains("\"name\":\"apply_patch\""));
        assert!(sse_body.contains("*** Begin Patch"));
        assert!(sse_body.contains("event: response.completed"));
        assert!(!sse_body.contains("\"type\":\"function_call\""));

        // 2) 上游 error 事件 → response.failed（不得靜默 completed）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "glm-test",
                "instructions": "be nice",
                "input": [{"type": "message", "role": "user", "content": "fail"}],
                "tools": tools,
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let sse_body = r.text().await.unwrap();
        assert!(sse_body.contains("event: response.failed"), "{sse_body}");
        assert!(sse_body.contains("upstream boom"));
        assert!(sse_body.contains("rate_limit_error"));
        assert!(!sse_body.contains("response.completed"));
        gw.abort();
    }

    /// 跨來源路由 e2e：模型登記在來源 B、Key 綁來源 A → 請求改走 B（帶 B 的金鑰）；
    /// 兩邊都沒有的模型 → 400 防呆（A 已登記模型時）。
    #[tokio::test]
    async fn e2e_model_reroute_cross_provider() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("rr.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            // A：Key 綁定來源；基址故意不可達（若未改走 B 會 502，作金絲雀）
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pa','universal','openai-chat','http://127.0.0.1:9','up-key-a','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pa: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pa'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'a-model', 'a-model', 'unset', 0, 1)",
                [pa],
            )
            .unwrap();
        }
        // 假上游 B：回顯收到的模型與上游鑑權（直通按入站路徑原樣轉發）
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(
                |headers: axum::http::HeaderMap,
                 body: axum::Json<serde_json::Value>| async move {
                    let auth = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    axum::Json(serde_json::json!({
                        "model": body.get("model").cloned().unwrap_or_default(),
                        "up_auth": auth,
                        "usage": {"prompt_tokens": 3, "completion_tokens": 1}
                    }))
                    .into_response()
                },
            ),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pb','universal','openai-chat',?1,'up-key-b','bearer','[]',1,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pb: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pb'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'or-model', 'or-model', 'unset', 0, 1)",
                [pb],
            )
            .unwrap();
            let pa: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pa'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "rr".into(),
                    provider_id: Some(pa),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) or-model 只登記在 B → 改走 B，帶 B 的金鑰
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "or-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["model"], "or-model");
        assert_eq!(body["up_auth"], "Bearer up-key-b", "應改走來源 B");
        // 2) 兩邊都沒有的模型 → 400 防呆（A 已登記啟用模型）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "nowhere-model"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        // 3) 記帳落在路由命中的來源 B
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (pid,): (i64,) = conn
            .query_row(
                "SELECT provider_id FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();
        let pb: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='pb'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pid, pb);
        gw.abort();
    }

    /// 直通 e2e：OpenAI 進 → display 名改寫為 actual 再轉發；
    /// 未映射模型命中跨來源防呆 400；記帳用實際送出的模型。
    #[tokio::test]
    async fn e2e_passthrough_resolves_display_to_actual() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pt.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('pt','codex','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pt'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'deepseek-v4-flash-0731', 'deepseek-ai/deepseek-v4-flash-0731', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假上游：回顯收到的模型名
        let fake = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                axum::Json(serde_json::json!({
                    "model": body.get("model").cloned().unwrap_or_default(),
                    "usage": {"prompt_tokens": 5, "completion_tokens": 1}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='pt'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='pt'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "pt".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) 顯示名進 → 上游收到 actual
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "deepseek-v4-flash-0731"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["model"], "deepseek-ai/deepseek-v4-flash-0731");
        // 2) 未映射模型 → 跨來源防呆 400，不轉發
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "gpt-9"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        // 3) 成功請求按實際模型記帳
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let m: String = conn
            .query_row(
                "SELECT model_raw FROM request_logs WHERE status=200 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m, "deepseek-ai/deepseek-v4-flash-0731");
        gw.abort();
    }

    /// 翻譯 e2e：Anthropic 進 → 假 OpenAI 出 → 客戶端收到 Anthropic 形狀 + 按實際模型記帳；
    /// 反向（OpenAI → Anthropic 渠道）明確 400。
    #[tokio::test]
    async fn e2e_anthropic_to_openai() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("tr.db");
        {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('tr','universal','openai-chat','PLACEHOLDER','up-key','bearer','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('an','universal','anthropic','http://127.0.0.1:1','k','anthropic','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='tr'", [], |r| r.get(0))
                .unwrap();
            conn.execute(
                "INSERT INTO provider_models (provider_id, display_name, actual_model, reasoning, ord, enabled)
                 VALUES (?1, 'claude-sonnet-4-6', 'deepseek-v4-flash', 'unset', 0, 1)",
                [pid],
            )
            .unwrap();
        }
        // 假 OpenAI 上游：斷言收到轉換後的請求
        let fake = axum::Router::new().route(
            "/chat/completions",
            axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
                assert_eq!(body["model"], "deepseek-v4-flash");
                assert_eq!(body["messages"][0]["role"], "system");
                assert_eq!(body["messages"][1]["content"], "hi");
                axum::Json(serde_json::json!({
                    "id": "chatcmpl-t1", "model": "deepseek-v4-flash",
                    "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 8, "completion_tokens": 2}
                }))
                .into_response()
            }),
        );
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let (secret, an_secret) = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "UPDATE providers SET base_url=?1 WHERE name='tr'",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let mk = |name: &str| {
                let pid: i64 = conn
                    .query_row("SELECT id FROM providers WHERE name=?1", [name], |r| {
                        r.get(0)
                    })
                    .unwrap();
                keys::create_key(
                    &conn,
                    &keys::KeyInput {
                        name: name.into(),
                        provider_id: Some(pid),
                        quota_tokens: -1,
                        rate_limit_qpm: 0,
                        allowed_models: vec![],
                        allowed_apps: vec![],
                        expires_at: None,
                    },
                )
                .unwrap()
                .secret
            };
            (mk("tr"), mk("an"))
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();

        // 1) Anthropic 進 → Anthropic 出
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/messages"))
            .bearer_auth(&secret)
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-sonnet-4-6",
                "max_tokens": 16,
                "system": "sys",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["type"], "message");
        assert_eq!(body["model"], "claude-sonnet-4-6", "回顯請求模型");
        assert_eq!(body["content"][0]["text"], "ok");
        assert_eq!(body["usage"]["input_tokens"], 8);
        assert_eq!(body["usage"]["output_tokens"], 2);
        // 按實際模型記帳
        let conn = crate::db::open_and_ensure(&db_path).unwrap();
        let (m, itok, otok): (String, i64, i64) = conn
            .query_row(
                "SELECT model_raw, in_tok, out_tok FROM request_logs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((m.as_str(), itok, otok), ("deepseek-v4-flash", 8, 2));

        // 2) 反向（OpenAI 請求打 Anthropic 渠道）→ 明確 400
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/chat/completions"))
            .bearer_auth(&an_secret)
            .json(&serde_json::json!({"model": "x", "messages": []}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);

        gw.abort();
    }

    #[test]
    fn strip_unsupported_tools_keeps_function() {
        let body = serde_json::json!({
            "model": "deepseek-v4-flash",
            "tools": [
                {"type": "function", "function": {"name": "web_search", "parameters": {}}},
                {"type": "custom", "name": "computer_use", "parameters": {}},
                {"type": "custom", "name": "shell", "parameters": {}}
            ],
            "tool_choice": {"type": "function", "function": {"name": "web_search"}},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let tools = v["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        // tool_choice 仍存在（引用的 tool 還在）
        assert!(v.get("tool_choice").is_some());
    }

    #[test]
    fn strip_unsupported_tools_removes_all() {
        let body = serde_json::json!({
            "model": "m",
            "tools": [
                {"type": "custom", "name": "a"},
                {"type": "shell", "name": "b"}
            ],
            "tool_choice": "required",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("tools").is_none());
        assert!(v.get("tool_choice").is_none());
    }

    #[test]
    fn strip_unsupported_tools_no_tools() {
        let body = serde_json::json!({"model": "m", "messages": []});
        let bytes = serde_json::to_vec(&body).unwrap();
        // 沒有 tools 欄位，原樣返回
        assert!(strip_unsupported_tools(&bytes).is_none());
    }

    #[test]
    fn passthrough_chat_sanitize_repairs_and_noops() {        // 孤兒 tool_calls（舊中斷 session）→ 補佔位
        let broken = br#"{"model":"m","messages":[
            {"role":"user","content":"hi"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"f","arguments":"{}"}}]}
        ]}"#;
        let out = sanitize_passthrough_chat_body(broken, InFmt::OpenAi, false).expect("應修復");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "c1");
        // 有效歷史一字不動
        let ok_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
        assert!(sanitize_passthrough_chat_body(ok_body, InFmt::OpenAi, false).is_none());
        // 非 chat / 已翻譯不碰
        assert!(sanitize_passthrough_chat_body(broken, InFmt::Anthropic, false).is_none());
        assert!(sanitize_passthrough_chat_body(broken, InFmt::OpenAi, true).is_none());
        assert!(sanitize_passthrough_chat_body(b"not json", InFmt::OpenAi, false).is_none());
    }

    // ───────── B1：encrypted_content 剝離（Responses 直通路徑）─────────

    #[test]
    fn strip_encrypted_content_removes_nested_occurrences() {
        // 第三方 responses 端點會以「encrypted_content was not issued to this
        // caller」400 拒收，故直通前必須剝離所有副本（含巢狀）。
        let body = br#"{"model":"m","input":[
            {"type":"reasoning","encrypted_content":"SECRET","summary":[]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}],
             "meta":{"encrypted_content":"ALSO_SECRET"}}
        ],"store":false}"#;
        let out = strip_encrypted_content(body).expect("應命中並剝離");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "仍有殘留: {s}");
        assert!(!s.contains("SECRET"), "簽章值未移除: {s}");
        // 其餘欄位一字不動
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["model"], "m");
        assert_eq!(v["store"], false);
        assert_eq!(v["input"].as_array().unwrap().len(), 2);
        assert_eq!(v["input"][0]["type"], "reasoning");
        assert_eq!(v["input"][1]["content"][0]["text"], "hi");
    }

    #[test]
    fn strip_encrypted_content_is_noop_when_absent() {
        // 未命中必須回 None（呼叫端據此判斷「原文一字不動」）
        assert!(strip_encrypted_content(br#"{"model":"m","input":"hi"}"#).is_none());
    }

    #[test]
    fn strip_encrypted_content_handles_arrays_and_deep_nesting() {
        let body =
            br#"{"a":[[{"encrypted_content":"x"}]],"b":{"c":{"d":{"encrypted_content":"y"}}}}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "深層殘留: {s}");
    }

    #[test]
    fn strip_encrypted_content_leaves_similar_keys_alone() {
        // 只剝離精確鍵名，不得誤傷其他含相似字串的鍵
        let body = br#"{"my_encrypted_content_x":1,"encrypted_content":"z"}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("encrypted_content").is_none());
        assert_eq!(v["my_encrypted_content_x"], 1, "相似鍵名被誤刪");
    }

    #[test]
    fn strip_encrypted_content_ignores_non_json() {
        assert!(strip_encrypted_content(b"not json").is_none());
        assert!(strip_encrypted_content(b"").is_none());
    }

    // ───────── B5：格式矩陣窮舉（30 格全釘死）─────────

    #[test]
    fn trans_kind_matrix_is_fully_pinned() {
        use InFmt::*;
        use TargetFmt as T;
        const NONE: Option<TransKind> = Some(TransKind::None);
        const A2C: Option<TransKind> = Some(TransKind::AnthropicToChat);
        const R2C: Option<TransKind> = Some(TransKind::ResponsesToChat);
        // 5 種入站 × 6 種渠道協議 = 30 格，全部明列。
        // 這份表就是行為契約：任何分支被重排或漏改，這裡立刻失敗。
        let table: &[(InFmt, T, Option<TransKind>)] = &[
            // Anthropic 入站
            (Anthropic, T::OpenAiChat, A2C),
            (Anthropic, T::OpenAiResponses, None),
            (Anthropic, T::Mixed, A2C),
            (Anthropic, T::Anthropic, NONE),
            (Anthropic, T::Gemini, None),
            (Anthropic, T::Unknown, None),
            // Responses 入站（Codex）
            (Responses, T::OpenAiChat, R2C),
            (Responses, T::OpenAiResponses, NONE),
            (Responses, T::Mixed, R2C),
            (Responses, T::Anthropic, None),
            (Responses, T::Gemini, None),
            (Responses, T::Unknown, NONE),
            // OpenAI Chat 入站
            (OpenAi, T::OpenAiChat, NONE),
            (OpenAi, T::OpenAiResponses, None), // B6：原為靜默直通
            (OpenAi, T::Mixed, NONE),
            (OpenAi, T::Anthropic, None),
            (OpenAi, T::Gemini, None),
            (OpenAi, T::Unknown, NONE),
            // Gemini 原生入站
            (Gemini, T::OpenAiChat, None),
            (Gemini, T::OpenAiResponses, None),
            (Gemini, T::Mixed, None),
            (Gemini, T::Anthropic, None),
            (Gemini, T::Gemini, NONE),
            (Gemini, T::Unknown, None),
            // 未知入站（中性路徑，如 /v1/models）
            (Unknown, T::OpenAiChat, NONE),
            (Unknown, T::OpenAiResponses, NONE),
            (Unknown, T::Mixed, NONE),
            (Unknown, T::Anthropic, NONE),
            (Unknown, T::Gemini, None),
            (Unknown, T::Unknown, NONE),
        ];
        assert_eq!(table.len(), 30, "矩陣必須窮舉 5×6 共 30 格");
        for (i, t, want) in table {
            let got = resolve_trans_kind(*i, *t);
            match want {
                Some(k) => assert_eq!(got.ok(), Some(*k), "({i:?}, {t:?}) 翻譯類型不符"),
                None => assert!(got.is_err(), "({i:?}, {t:?}) 應被明確拒絕"),
            }
        }
    }

    #[test]
    fn b6_chat_to_responses_endpoint_errors_loudly() {
        // 原實作落入 `_ => TransKind::None`，把 chat 請求原樣打到 responses 端點，
        // 必然失敗且訊息來自上游、難以理解。改為網關自己明確報錯。
        let e = resolve_trans_kind(InFmt::OpenAi, TargetFmt::OpenAiResponses)
            .expect_err("應明確拒絕而非靜默直通");
        assert_eq!(e, E_CHAT_TO_RESPONSES);
        assert!(e.contains("Responses"), "訊息應指出協議不符: {e}");
    }

    #[test]
    fn matrix_preserves_original_error_messages() {
        // 逐格核對既有 400 訊息語義不變（含原實作的分支優先序）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::OpenAiResponses).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        // anthropic 入站的訊息優先於「gemini 渠道」訊息（原實作順序語義）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::Gemini).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Responses, TargetFmt::Anthropic).unwrap_err(),
            E_REVERSE_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Gemini, TargetFmt::OpenAiChat).unwrap_err(),
            E_GEMINI_IN_ONLY
        );
        assert_eq!(
            resolve_trans_kind(InFmt::OpenAi, TargetFmt::Gemini).unwrap_err(),
            E_GEMINI_OUT_ONLY
        );
    }

    #[test]
    fn in_fmt_from_path_matches_legacy_detection() {
        // 與原 detect_in_format 的判定完全一致
        assert_eq!(InFmt::from_path("/v1/messages"), InFmt::Anthropic);
        assert_eq!(InFmt::from_path("/v1/responses"), InFmt::Responses);
        assert_eq!(InFmt::from_path("/v1/chat/completions"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/models"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/embeddings"), InFmt::OpenAi);
        assert_eq!(
            InFmt::from_path("/v1beta/models/gemini-2.5-pro:generateContent"),
            InFmt::Gemini
        );
        assert_eq!(
            InFmt::from_path("/v1beta/models/x:streamGenerateContent"),
            InFmt::Gemini
        );
        // 中性路徑 → Unknown（透傳，不誤傷）
        assert_eq!(InFmt::from_path("/v1/health"), InFmt::Unknown);
    }

    #[test]
    fn target_fmt_from_db_covers_every_ui_value() {
        // 前端 Providers.tsx 的 API_FORMATS 必須全部被辨識，
        // 否則會落入 Unknown 而被拒服務。
        for f in [
            "openai-chat",
            "openai-responses",
            "mixed",
            "anthropic",
            "gemini",
        ] {
            assert_ne!(TargetFmt::from_db(f), TargetFmt::Unknown, "{f} 未被辨識");
            // as_str 應可往返
            assert_eq!(TargetFmt::from_db(f).as_str(), f);
        }
        assert_eq!(TargetFmt::from_db("typo"), TargetFmt::Unknown);
        assert_eq!(TargetFmt::from_db(""), TargetFmt::Unknown);
    }

    /// 嚴格上游 400 unknown field → 剝離重發一次成功，且同渠道後續請求預先剝離。
    #[tokio::test]
    async fn e2e_strict_upstream_retry_strip() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let hits = Arc::new(AtomicUsize::new(0));
        let fake = {
            let hits = hits.clone();
            axum::Router::new().route(
                "/v1/responses",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let hits = hits.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            if body.get("verbosity").is_some() {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "error": {"message": "json: unknown field \"verbosity\" Request id: 1",
                                            "type": "BadRequest", "code": "InvalidParameter"}
                                    })),
                                )
                                    .into_response();
                            }
                            axum::Json(serde_json::json!({
                                "id": "resp-1", "model": "m",
                                "output": [{"type": "message", "content": [{"type": "output_text", "text": "ok"}]}],
                                "usage": {"input_tokens": 3, "output_tokens": 1}
                            }))
                            .into_response()
                        }
                    },
                ),
            )
        };
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('s','universal','openai-responses',?1,'k','bearer','[]',0,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='s'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "s".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        // Codex 式請求自帶 verbosity：首發 400 → 剝離重發 → 200
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": "hi", "verbosity": "low"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        // 第二次同樣請求只打一次（記憶生效）
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": "hi", "verbosity": "low"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        gw.abort();
    }

    /// 並行 function_call 經 responses→chat 翻譯後必須是一條 assistant 帶全部
    /// tool_calls（逐條拆分會被嚴格上游以「其後非 tool 消息」拒收，見 Zen 400）。
    #[tokio::test]
    async fn e2e_parallel_calls_forwarded_merged() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let captured: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let fake = {
            let captured = captured.clone();
            axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let captured = captured.clone();
                        async move {
                            *captured.lock().unwrap() = Some(body.0);
                            axum::Json(serde_json::json!({
                                "id": "chatcmpl-1", "object": "chat.completion",
                                "created": 1, "model": "m",
                                "choices": [{"index": 0,
                                    "message": {"role": "assistant", "content": "ok"},
                                    "finish_reason": "stop"}],
                                "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                            }))
                            .into_response()
                        }
                    },
                ),
            )
        };
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('s','universal','openai-chat',?1,'k','bearer','[]',0,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='s'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "s".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        // Codex 舊 session 重放：連發 3 個 function_call + 3 個 output
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({"model": "m", "input": [
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "function_call", "call_id": "k0", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                {"type": "function_call", "call_id": "k2", "name": "f", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "k0", "output": "r0"},
                {"type": "function_call_output", "call_id": "k1", "output": "r1"},
                {"type": "function_call_output", "call_id": "k2", "output": "r2"}
            ]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let fwd = captured.lock().unwrap().clone().expect("上游應收到請求");
        let msgs = fwd["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 5, "應為 user+assistant+tool*3，實際：{msgs:?}");
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[1]["tool_calls"].as_array().unwrap().len(), 3);
        assert_eq!(msgs[1]["tool_calls"][0]["id"], "k0");
        assert_eq!(msgs[1]["tool_calls"][2]["id"], "k2");
        assert_eq!(msgs[2]["tool_call_id"], "k0");
        assert_eq!(msgs[4]["tool_call_id"], "k2");
        gw.abort();
    }

    #[tokio::test]
    async fn e2e_strict_upstream_rejects_empty_non_assistant_content() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let captured: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let fake = {
            let captured = captured.clone();
            axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let captured = captured.clone();
                        async move {
                            let invalid = body["messages"]
                                .as_array()
                                .is_some_and(|msgs| {
                                    msgs.iter().any(|m| {
                                        m.get("role").and_then(|r| r.as_str()) != Some("assistant")
                                            && m.get("content")
                                                .and_then(|c| c.as_str())
                                                .is_none_or(str::is_empty)
                                    })
                                });
                            *captured.lock().unwrap() = Some(body.0.clone());
                            if invalid {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "error": {
                                            "code": "400",
                                            "message": "message content is required for non-assistant roles",
                                            "param": "messages",
                                            "type": "invalid_request_error"
                                        }
                                    })),
                                )
                                    .into_response();
                            }
                            axum::Json(serde_json::json!({
                                "id": "chatcmpl-1", "object": "chat.completion",
                                "created": 1, "model": "m",
                                "choices": [{"index": 0,
                                    "message": {"role": "assistant", "content": "ok"},
                                    "finish_reason": "stop"}],
                                "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                            }))
                            .into_response()
                        }
                    },
                ),
            )
        };
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });
        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('s','universal','openai-chat',?1,'k','bearer','[]',0,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='s'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "s".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let response = Client::new()
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "m",
                "input": [
                    {"type": "message", "role": "user", "content": "hi"},
                    {"type": "function_call", "call_id": "k1", "name": "f", "arguments": "{}"},
                    {"type": "function_call_output", "call_id": "k1", "output": ""}
                ]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = captured.lock().unwrap().clone().unwrap();
        for message in body["messages"].as_array().unwrap() {
            if message["role"] != "assistant" {
                assert!(!message["content"].as_str().unwrap_or("").is_empty(), "{body}");
            }
        }
        gw.abort();
    }

    /// OpenRouter 口徑：Codex 自帶 prompt_cache_key，翻译後 chat 體首發被
    /// `Unsupported parameter(s)` 400 → 剝離重發 → 200（2026-09-24 codex 實例）。
    #[tokio::test]
    async fn e2e_openrouter_prompt_cache_key_strip() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("s.db");
        let hits = Arc::new(AtomicUsize::new(0));
        let last_body: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        let fake = {
            let hits = hits.clone();
            let last_body = last_body.clone();
            axum::Router::new().route(
                "/chat/completions",
                axum::routing::post(
                    move |body: axum::Json<serde_json::Value>| {
                        let hits = hits.clone();
                        let last_body = last_body.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            *last_body.lock().unwrap() = Some(body.0.clone());
                            if body.get("prompt_cache_key").is_some() {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "message": "Validation: Unsupported parameter(s): `prompt_cache_key`",
                                        "type": "Bad Request", "code": 400
                                    })),
                                )
                                    .into_response();
                            }
                            axum::Json(serde_json::json!({
                                "id": "chatcmpl-1", "object": "chat.completion",
                                "created": 1, "model": "m",
                                "choices": [{"index": 0,
                                    "message": {"role": "assistant", "content": "ok"},
                                    "finish_reason": "stop"}],
                                "usage": {"prompt_tokens": 1, "completion_tokens": 1}
                            }))
                            .into_response()
                        }
                    },
                ),
            )
        };
        let fake_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake_port = fake_l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(fake_l, fake).await.unwrap() });

        let secret = {
            let conn = crate::db::open_and_ensure(&db_path).unwrap();
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
                 VALUES ('s','universal','openai-chat',?1,'k','bearer','[]',0,1,0,0)",
                [format!("http://127.0.0.1:{fake_port}")],
            )
            .unwrap();
            let pid: i64 = conn
                .query_row("SELECT id FROM providers WHERE name='s'", [], |r| r.get(0))
                .unwrap();
            keys::create_key(
                &conn,
                &keys::KeyInput {
                    name: "s".into(),
                    provider_id: Some(pid),
                    quota_tokens: -1,
                    rate_limit_qpm: 0,
                    allowed_models: vec![],
                    allowed_apps: vec![],
                    expires_at: None,
                },
            )
            .unwrap()
            .secret
        };
        let gw_l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_port = gw_l.local_addr().unwrap().port();
        let gw = tokio::spawn(serve(db_path.clone(), gw_l));
        let http = Client::new();
        let r = http
            .post(format!("http://127.0.0.1:{gw_port}/v1/responses"))
            .bearer_auth(&secret)
            .json(&serde_json::json!({
                "model": "m", "prompt_cache_key": "sess-1", "input": "hi"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        let fwd = last_body.lock().unwrap().clone().expect("上游應收到請求");
        assert!(fwd.get("prompt_cache_key").is_none(), "{fwd}");
        gw.abort();
    }
}
