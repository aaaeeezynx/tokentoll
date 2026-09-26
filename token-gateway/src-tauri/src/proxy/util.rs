//! 通用小工具：連線、模型名正規化、來源判定、標頭處理、路徑處理。
//!
//! 這裡放的是「不屬於任何單一階段、但多個階段都要用」的純函式。

use super::*;

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
pub(super) fn open_conn(db_path: &PathBuf) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

// ---------------------------------------------------------------- 用量 ---
// 用量結構與 OpenAI 讀取邏輯統一放在 translate（翻譯與代理共用）。

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
    pub(super) fn cut_full_date(s: &mut String) -> bool {
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
pub(super) fn bearer(headers: &HeaderMap) -> Option<String> {
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
pub(super) fn join_upstream(base: &str, incoming_path: &str) -> String {
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
pub(super) fn is_hop_header(name: &str) -> bool {
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
pub(super) fn should_inject_usage(translated: bool, incoming_path: &str) -> bool {
    translated || incoming_path.contains("/chat/completions")
}
/// 去掉 query 中的 `key` 參數（Google 式鑑權殘留，避免客戶端 key 上送）。
pub(super) fn strip_key_param(query: Option<&str>) -> Option<String> {
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
pub(super) fn model_from_path(path: &str) -> Option<String> {
    let last = path.rsplit('/').next()?;
    let (id, _) = last.split_once(':')?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}
