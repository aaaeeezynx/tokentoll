//! M5：歷史回填 — 離線解析本機會話並冪等寫入 `request_logs`。
//!
//! 來源有三：Claude Code（`~/.claude/projects/**/*.jsonl`，按 assistant 訊息計）、
//! Codex（`~/.codex/sessions/**/*.jsonl`，按 `token_usage_record` 計）、
//! OpenCode（`~/.local/share/opencode/opencode.db` 的 `session` 表，按會話計）。
//!
//! 寫入列標 `source='import'`、`import_path=來源文件`；key/provider 未知記 NULL
//! （OpenCode 會話按 `providerID` 名稱回填渠道，能對上才寫）。
//!
//! 冪等：`import_state` 記每文件 (mtime, size)／整庫指紋；
//! 指紋變化時按 `import_path` 刪除重插。與網關日誌的疑似重複
//! （同 app+模型+輸入輸出+時間 ±120s）跳過並計數。
//!
//! 費用：回填當下按全域/渠道定價快照計算並凍存；無價模型記 0 並列入摘要。

use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fsutil::now_ms;
use crate::proxy::normalize_model;
use crate::usage::{rate_cost, resolve_rates};

pub const IMPORT_SOURCE: &str = "import";
/// 與網關日誌疑似重複的時間容差（毫秒）。
const DUPE_WINDOW_MS: i64 = 120_000;

#[derive(Debug, Clone, Serialize)]
pub struct ScanTool {
    pub tool: String,
    pub files: usize,
    pub bytes: u64,
    /// 僅 opencode：會話數。
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolImportStat {
    pub tool: String,
    pub files: usize,
    pub rows: i64,
    pub tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnpricedModel {
    pub model: String,
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportSummary {
    pub files_scanned: usize,
    pub files_imported: usize,
    pub files_unchanged: usize,
    pub rows_added: i64,
    pub dupes_skipped: i64,
    pub empty_skipped: i64,
    pub bad_lines: i64,
    pub in_tok: i64,
    pub out_tok: i64,
    pub cost_usd: f64,
    pub per_tool: Vec<ToolImportStat>,
    pub unpriced_top: Vec<UnpricedModel>,
}

struct ParsedRow {
    app: &'static str,
    model_raw: String,
    provider_id: Option<i64>,
    in_tok: i64,
    out_tok: i64,
    cache_read: i64,
    cache_write: i64,
    ts_ms: i64,
    import_path: String,
}

// ---------------------------------------------------------------- 工具函數 ---

fn num(v: &serde_json::Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|n| n.as_i64())
        .unwrap_or(0)
        .max(0)
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// RFC3339 → 毫秒時間戳；失敗回 None。
fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.timestamp_millis())
}

fn file_fp(path: &Path) -> (i64, i64) {
    let md = std::fs::metadata(path).ok();
    let size = md.as_ref().map(|m| m.len() as i64).unwrap_or(0);
    let mtime = md
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (mtime, size)
}

/// 遞迴收集目錄下所有 `.jsonl`（大小寫不敏感）。
fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            collect_jsonl_files(&p, out);
        } else if p
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("jsonl"))
        {
            out.push(p);
        }
    }
}

// ---------------------------------------------------------------- 掃描 ---

/// 各工具待掃描的來源（不存在的目錄回空列表，不報錯）。
pub fn scan_history(home: &Path) -> Vec<ScanTool> {
    let mut tools = Vec::new();
    let mut claude_files = Vec::new();
    collect_jsonl_files(&home.join(".claude").join("projects"), &mut claude_files);
    tools.push(summarize("claude", claude_files, 0));
    let mut codex_files = Vec::new();
    collect_jsonl_files(&home.join(".codex").join("sessions"), &mut codex_files);
    tools.push(summarize("codex", codex_files, 0));
    // opencode：單庫 + 會話數（唯讀 COUNT，失敗當 0）
    let oc = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    let (files, bytes, sessions) = if oc.exists() {
        let n = rusqlite::Connection::open_with_flags(&oc, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| {
                c.query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
                    .ok()
            })
            .unwrap_or(0);
        (1, std::fs::metadata(&oc).map(|m| m.len()).unwrap_or(0), n as usize)
    } else {
        (0, 0, 0)
    };
    tools.push(ScanTool {
        tool: "opencode".into(),
        files,
        bytes,
        sessions,
    });
    tools
}

fn summarize(tool: &str, files: Vec<PathBuf>, sessions: usize) -> ScanTool {
    let bytes = files
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();
    ScanTool {
        tool: tool.into(),
        files: files.len(),
        bytes,
        sessions,
    }
}

// ---------------------------------------------------------------- 解析 ---

/// Claude Code：assistant 行的 message.usage。
fn parse_claude_file(text: &str, import_path: &str, bad: &mut i64) -> Vec<ParsedRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                *bad += 1;
                continue;
            }
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => {
                *bad += 1;
                continue;
            }
        };
        let usage = match msg.get("usage") {
            Some(u) if u.is_object() => u,
            _ => continue, // 無用量（如 summary 行）直接略過
        };
        let Some(ts) = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_ts)
        else {
            *bad += 1;
            continue;
        };
        rows.push(ParsedRow {
            app: "claude",
            model_raw: str_field(msg, "model"),
            provider_id: None,
            in_tok: num(usage, "input_tokens"),
            out_tok: num(usage, "output_tokens"),
            cache_read: num(usage, "cache_read_input_tokens"),
            cache_write: num(usage, "cache_creation_input_tokens"),
            ts_ms: ts,
            import_path: import_path.to_string(),
        });
    }
    rows
}

/// Codex rollout：兩遍掃描。第一遍建 turn→model 表，第二遍取 token_usage_record。
fn parse_codex_file(text: &str, import_path: &str, bad: &mut i64) -> Vec<ParsedRow> {
    let mut turn_model: HashMap<String, String> = HashMap::new();
    let mut root_model: HashMap<String, String> = HashMap::new();
    let mut file_model = String::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("turn_context") {
            continue;
        }
        let p = match v.get("payload") {
            Some(p) => p,
            None => continue,
        };
        let model = str_field(p, "model");
        if model.is_empty() {
            continue;
        }
        if file_model.is_empty() {
            file_model = model.clone();
        }
        let tid = str_field(p, "turn_id");
        if !tid.is_empty() {
            turn_model.insert(tid, model.clone());
        }
        let rid = str_field(p, "root_turn_id");
        if !rid.is_empty() {
            root_model.entry(rid).or_insert(model);
        }
    }
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                *bad += 1;
                continue;
            }
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("token_usage_record") {
            continue;
        }
        let (p, usage) = match (v.get("payload"), v.get("payload").and_then(|p| p.get("usage"))) {
            (Some(p), Some(u)) if u.is_object() => (p, u),
            _ => {
                *bad += 1;
                continue;
            }
        };
        let Some(ts) = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_ts)
        else {
            *bad += 1;
            continue;
        };
        let tid = str_field(p, "turn_id");
        let rid = str_field(p, "root_turn_id");
        let model = turn_model
            .get(&tid)
            .or_else(|| root_model.get(&rid))
            .cloned()
            .unwrap_or_else(|| file_model.clone());
        rows.push(ParsedRow {
            app: "codex",
            model_raw: model,
            provider_id: None,
            in_tok: num(usage, "input_tokens"),
            out_tok: num(usage, "output_tokens"),
            cache_read: num(usage, "cached_input_tokens"),
            cache_write: num(usage, "cache_write_input_tokens"),
            ts_ms: ts,
            import_path: import_path.to_string(),
        });
    }
    rows
}

/// OpenCode：session 表一行一會話；model 是 JSON（取 id）；providerID 回填渠道。
fn parse_opencode_db(
    conn: &Connection,
    provider_map: &HashMap<String, i64>,
    bad: &mut i64,
) -> Vec<ParsedRow> {
    let mut stmt = match conn.prepare(
        "SELECT id, model, time_created, time_updated,
                tokens_input, tokens_output, tokens_reasoning,
                tokens_cache_read, tokens_cache_write
         FROM session",
    ) {
        Ok(s) => s,
        Err(_) => {
            *bad += 1;
            return Vec::new();
        }
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
        ))
    });
    let mut out = Vec::new();
    let rows = match rows {
        Ok(r) => r,
        Err(_) => {
            *bad += 1;
            return out;
        }
    };
    for r in rows.filter_map(|r| r.ok()) {
        let (id, model_json, tc, tu, i, o, rea, cr, cw) = r;
        let (model_raw, pid) = match model_json.as_deref() {
            Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(m) => {
                    let mid = m
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let prov = m
                        .get("providerID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_lowercase();
                    let alias = if prov == "opencode" { "zen" } else { prov.as_str() };
                    (mid, provider_map.get(alias).copied())
                }
                Err(_) => {
                    *bad += 1;
                    continue;
                }
            },
            None => {
                *bad += 1;
                continue;
            }
        };
        if model_raw.is_empty() {
            *bad += 1;
            continue;
        }
        // reasoning 併入輸出（與網關口徑一致：輸出 tokens 含思考）。
        out.push(ParsedRow {
            app: "opencode",
            model_raw,
            provider_id: pid,
            in_tok: i.max(0),
            out_tok: (o + rea).max(0),
            cache_read: cr.max(0),
            cache_write: cw.max(0),
            ts_ms: if tu > 0 { tu } else { tc },
            import_path: format!("opencode:{id}"),
        });
    }
    out
}

// ---------------------------------------------------------------- 寫入 ---

fn provider_name_map(conn: &Connection) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, name FROM providers") {
        if let Ok(rows) = stmt.query_map([], |r| {
            let id: i64 = r.get(0)?;
            let name: String = r.get(1)?;
            Ok((id, name))
        }) {
            for r in rows.filter_map(|r| r.ok()) {
                map.insert(r.1.trim().to_lowercase(), r.0);
            }
        }
    }
    map
}

/// 與網關日誌疑似重複（同 app+模型+輸入輸出+時間窗）→ 跳過，避免雙計。
fn is_gateway_dupe(
    conn: &Connection,
    app: &str,
    model_norm: &str,
    in_tok: i64,
    out_tok: i64,
    ts_ms: i64,
) -> bool {
    conn.query_row(
        "SELECT 1 FROM request_logs
         WHERE source = 'gateway' AND app = ?1 AND model_norm = ?2
           AND in_tok = ?3 AND out_tok = ?4
           AND ts BETWEEN ?5 AND ?6 LIMIT 1",
        params![
            app,
            model_norm,
            in_tok,
            out_tok,
            ts_ms - DUPE_WINDOW_MS,
            ts_ms + DUPE_WINDOW_MS
        ],
        |_| Ok(()),
    )
    .is_ok()
}

fn state_fingerprint(conn: &Connection, tool: &str, path: &str) -> Option<(i64, i64)> {
    conn.query_row(
        "SELECT fp1, fp2 FROM import_state WHERE tool = ?1 AND path = ?2",
        params![tool, path],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

/// 任一層級存在定價行（全域 / 渠道 / 時段，含 `-MMDD` 父級）即視為已定價
///（即使全零，如免費模型），不再列入未定價榜。
fn has_any_pricing(conn: &Connection, provider_id: Option<i64>, norm: &str) -> bool {
    let mut cur = norm.trim().to_lowercase();
    for _ in 0..4 {
        if conn
            .query_row("SELECT 1 FROM pricing WHERE model_norm = ?1", [&cur], |_| {
                Ok(())
            })
            .is_ok()
        {
            return true;
        }
        if let Some(pid) = provider_id {
            if conn
                .query_row(
                    "SELECT 1 FROM provider_pricing WHERE provider_id = ?1 AND model_norm IN (?2, '*')",
                    params![pid, cur],
                    |_| Ok(()),
                )
                .is_ok()
            {
                return true;
            }
            if conn
                .query_row(
                    "SELECT 1 FROM pricing_periods WHERE provider_id = ?1 AND model_norm IN (?2, '*')",
                    params![pid, cur],
                    |_| Ok(()),
                )
                .is_ok()
            {
                return true;
            }
        }
        match crate::usage::strip_date_tail(&cur) {
            Some(p) => cur = p,
            None => break,
        }
    }
    false
}

/// 清理該工具已不在來源中的孤兒回填行（文件改名/會話刪除），回傳刪除數。
fn gc_orphans(conn: &Connection, app: &str, keep: &std::collections::HashSet<String>) -> Result<i64, String> {
    let existing: Vec<String> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT DISTINCT import_path FROM request_logs WHERE source = '{IMPORT_SOURCE}' AND app = ?1"
            ))
            .map_err(|e| e.to_string())?;
        let par = [app];
        let mapped = stmt
            .query_map(par, |r| r.get(0))
            .map_err(|e| e.to_string())?;
        mapped.filter_map(|r| r.ok()).collect()
    };
    let mut n = 0i64;
    for p in existing {
        if p.is_empty() || keep.contains(&p) {
            continue;
        }
        n += conn
            .execute(
                &format!(
                    "DELETE FROM request_logs WHERE source = '{IMPORT_SOURCE}' AND import_path = ?1"
                ),
                [&p],
            )
            .map_err(|e| e.to_string())? as i64;
    }
    Ok(n)
}
/// 回傳 (rows, in, out, cost, dupes, empty)。
#[allow(clippy::too_many_arguments)]
fn commit_rows(
    conn: &Connection,
    tool: &str,
    path: &str,
    fp: (i64, i64),
    rows: &[ParsedRow],
    delete_paths: &[String],
    unpriced: &mut HashMap<String, i64>,
) -> Result<(i64, i64, i64, f64, i64, i64), String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    {
        let mut del = tx
            .prepare("DELETE FROM request_logs WHERE import_path = ?1")
            .map_err(|e| e.to_string())?;
        for d in delete_paths {
            del.execute([d]).map_err(|e| e.to_string())?;
        }
    }
    let (mut n, mut itok, mut otok, mut cost, mut dupes, mut empty) = (0i64, 0i64, 0i64, 0.0, 0i64, 0i64);
    {
        let mut ins = tx
            .prepare(&format!(
                "INSERT INTO request_logs
                 (ts, key_id, app, provider_id, model_raw, model_norm,
                  in_tok, out_tok, cache_read, cache_write, cost_usd,
                  latency_ms, status, is_stream, source, import_path)
                 VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 200, 0, '{IMPORT_SOURCE}', ?11)"
            ))
            .map_err(|e| e.to_string())?;
        for r in rows {
            if r.ts_ms <= 0 {
                empty += 1;
                continue;
            }
            let toks = r.in_tok + r.out_tok + r.cache_read + r.cache_write;
            if toks <= 0 {
                empty += 1;
                continue;
            }
            let norm = normalize_model(&r.model_raw);
            if is_gateway_dupe(&tx, r.app, &norm, r.in_tok, r.out_tok, r.ts_ms) {
                dupes += 1;
                continue;
            }
            let rates = resolve_rates(&tx, r.provider_id, &norm, r.ts_ms);
            let c = rate_cost(&rates, r.in_tok, r.out_tok, r.cache_read, r.cache_write);
            if c == 0.0 && !norm.is_empty() && !has_any_pricing(&tx, r.provider_id, &norm) {
                *unpriced.entry(norm.clone()).or_insert(0) += toks;
            }
            ins.execute(params![
                r.ts_ms,
                r.app,
                r.provider_id,
                r.model_raw,
                norm,
                r.in_tok,
                r.out_tok,
                r.cache_read,
                r.cache_write,
                c,
                r.import_path,
            ])
            .map_err(|e| e.to_string())?;
            n += 1;
            itok = itok.saturating_add(r.in_tok);
            otok = otok.saturating_add(r.out_tok);
            cost += c;
        }
    }
    tx.execute(
        "INSERT INTO import_state (tool, path, fp1, fp2, rows_added, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(tool, path) DO UPDATE SET
           fp1 = excluded.fp1, fp2 = excluded.fp2,
           rows_added = excluded.rows_added, imported_at = excluded.imported_at",
        params![tool, path, fp.0, fp.1, n, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok((n, itok, otok, cost, dupes, empty))
}

// ---------------------------------------------------------------- 入口 ---

/// 回填主入口。`tools` 子集自 {"claude","codex","opencode"}；空 = 全不做。
/// 文件類按 (mtime, size) 冪等；opencode 整庫單指紋（會話增改即全量重導，
/// 量小無壓力）；每工具結束時清理來源已消失的孤兒行。
pub fn import_history(conn: &Connection, tools: &[String]) -> Result<ImportSummary, String> {
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .map_err(|e| e.to_string())?;
    let home = dirs::home_dir().ok_or("找不到用戶主目錄".to_string())?;
    let mut sum = ImportSummary::default();
    let mut unpriced: HashMap<String, i64> = HashMap::new();
    let want = |t: &str| tools.iter().any(|x| x == t);
    // 小記帳器：(rows, in, out, cost, dupes, empty) → 匯總 + 工具統計
    macro_rules! acc {
        ($st:expr, $t:expr) => {{
            let (n, i, o, c, d, e) = $t;
            $st.rows += n;
            $st.tokens = $st.tokens.saturating_add(i + o);
            $st.cost_usd += c;
            sum.rows_added += n;
            sum.dupes_skipped += d;
            sum.empty_skipped += e;
            sum.in_tok = sum.in_tok.saturating_add(i);
            sum.out_tok = sum.out_tok.saturating_add(o);
            sum.cost_usd += c;
        }};
    }

    if want("claude") {
        let mut files = Vec::new();
        collect_jsonl_files(&home.join(".claude").join("projects"), &mut files);
        let mut st = ToolImportStat {
            tool: "claude".into(),
            files: 0,
            rows: 0,
            tokens: 0,
            cost_usd: 0.0,
        };
        let mut keep = std::collections::HashSet::new();
        for f in &files {
            sum.files_scanned += 1;
            let path = f.to_string_lossy().to_string();
            keep.insert(path.clone());
            let fp = file_fp(f);
            if state_fingerprint(conn, "claude", &path) == Some(fp) {
                sum.files_unchanged += 1;
                continue;
            }
            let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
            let rows = parse_claude_file(&text, &path, &mut sum.bad_lines);
            let del = vec![path.clone()];
            acc!(
                st,
                commit_rows(conn, "claude", &path, fp, &rows, &del, &mut unpriced)?
            );
            sum.files_imported += 1;
            st.files += 1;
        }
        gc_orphans(conn, "claude", &keep)?;
        sum.per_tool.push(st);
    }

    if want("codex") {
        let mut files = Vec::new();
        collect_jsonl_files(&home.join(".codex").join("sessions"), &mut files);
        let mut st = ToolImportStat {
            tool: "codex".into(),
            files: 0,
            rows: 0,
            tokens: 0,
            cost_usd: 0.0,
        };
        let mut keep = std::collections::HashSet::new();
        for f in &files {
            sum.files_scanned += 1;
            let path = f.to_string_lossy().to_string();
            keep.insert(path.clone());
            let fp = file_fp(f);
            if state_fingerprint(conn, "codex", &path) == Some(fp) {
                sum.files_unchanged += 1;
                continue;
            }
            let text = std::fs::read_to_string(f).map_err(|e| e.to_string())?;
            let rows = parse_codex_file(&text, &path, &mut sum.bad_lines);
            let del = vec![path.clone()];
            acc!(
                st,
                commit_rows(conn, "codex", &path, fp, &rows, &del, &mut unpriced)?
            );
            sum.files_imported += 1;
            st.files += 1;
        }
        gc_orphans(conn, "codex", &keep)?;
        sum.per_tool.push(st);
    }

    if want("opencode") {
        let oc = home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db");
        let mut st = ToolImportStat {
            tool: "opencode".into(),
            files: 0,
            rows: 0,
            tokens: 0,
            cost_usd: 0.0,
        };
        if oc.exists() {
            sum.files_scanned += 1;
            let oc_conn = Connection::open_with_flags(&oc, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?;
            let pmap = provider_name_map(conn);
            let rows = parse_opencode_db(&oc_conn, &pmap, &mut sum.bad_lines);
            let keep: std::collections::HashSet<String> =
                rows.iter().map(|r| r.import_path.clone()).collect();
            let fp = (
                rows.iter().map(|r| r.ts_ms).max().unwrap_or(0),
                rows.len() as i64,
            );
            if state_fingerprint(conn, "opencode", "opencode.db") == Some(fp) {
                sum.files_unchanged += 1;
            } else {
                let del: Vec<String> = keep.iter().cloned().collect();
                acc!(
                    st,
                    commit_rows(conn, "opencode", "opencode.db", fp, &rows, &del, &mut unpriced)?
                );
                sum.files_imported += 1;
                st.files += 1;
            }
            gc_orphans(conn, "opencode", &keep)?;
        }
        sum.per_tool.push(st);
    }

    let mut up: Vec<UnpricedModel> = unpriced
        .into_iter()
        .map(|(model, tokens)| UnpricedModel { model, tokens })
        .collect();
    up.sort_by_key(|u| std::cmp::Reverse(u.tokens));
    up.truncate(10);
    sum.unpriced_top = up;
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("m5.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn claude_parse_cache_split() {
        let text = concat!(
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-4-8\",\"usage\":{\"input_tokens\":10,\"output_tokens\":3,\"cache_read_input_tokens\":4,\"cache_creation_input_tokens\":1}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"},\"timestamp\":\"2026-09-10T01:02:04.000Z\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":2}},\"timestamp\":\"2026-09-10T01:03:00.000Z\"}\n",
            "not json\n",
        );
        let mut bad = 0;
        let rows = parse_claude_file(text, "f.jsonl", &mut bad);
        assert_eq!(rows.len(), 2);
        assert_eq!(bad, 1);
        assert_eq!(rows[0].model_raw, "claude-opus-4-8");
        assert_eq!(rows[0].in_tok, 10);
        assert_eq!(rows[0].cache_read, 4);
        assert_eq!(rows[0].cache_write, 1);
        assert_eq!(rows[1].model_raw, "");
        // 2026-09-10T01:02:03Z → 毫秒（與 chrono 對照）
        let expect = chrono::DateTime::parse_from_rfc3339("2026-09-10T01:02:03.000Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(rows[0].ts_ms, expect);
    }

    #[test]
    fn codex_parse_turn_join_and_fallback() {
        let text = concat!(
            "{\"timestamp\":\"2026-09-10T01:00:00.000Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"t1\",\"root_turn_id\":\"r1\",\"model\":\"deepseek-v4-flash\"}}\n",
            "{\"timestamp\":\"2026-09-10T01:00:01.000Z\",\"type\":\"token_usage_record\",\"payload\":{\"turn_id\":\"t1\",\"root_turn_id\":\"r1\",\"usage\":{\"input_tokens\":100,\"cached_input_tokens\":10,\"cache_write_input_tokens\":0,\"output_tokens\":20,\"reasoning_output_tokens\":5,\"total_tokens\":120}}}\n",
            "{\"timestamp\":\"2026-09-10T01:00:02.000Z\",\"type\":\"token_usage_record\",\"payload\":{\"turn_id\":\"tx\",\"root_turn_id\":\"rx\",\"usage\":{\"input_tokens\":7,\"output_tokens\":1}}}\n",
        );
        let mut bad = 0;
        let rows = parse_codex_file(text, "r.jsonl", &mut bad);
        assert_eq!(rows.len(), 2);
        assert_eq!(bad, 0);
        assert_eq!(rows[0].model_raw, "deepseek-v4-flash");
        assert_eq!(rows[0].in_tok, 100);
        assert_eq!(rows[0].cache_read, 10);
        // 未知 turn 回退到文件首模型
        assert_eq!(rows[1].model_raw, "deepseek-v4-flash");
    }

    #[test]
    fn import_idempotent_and_reimport_on_change() {
        let (_d, conn) = mem_db();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("s.jsonl");
        std::fs::write(
            &f,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":100,\"output_tokens\":10}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n",
        )
        .unwrap();
        // 首輪
        let rows = parse_claude_file(
            &std::fs::read_to_string(&f).unwrap(),
            &f.to_string_lossy(),
            &mut 0,
        );
        let dp = vec![f.to_string_lossy().to_string()];
        let (n, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n, 1);
        let c1: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c1, 1);
        // 同指紋重跑 → 呼叫端會跳過；此處驗 commit 本身可重入（刪舊插新）
        let (n2, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n2, 1);
        let c2: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c2, 1, "重插不應翻倍");
        // 追加一行 → 行數變 2
        std::fs::write(
            &f,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":100,\"output_tokens\":10}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":50,\"output_tokens\":5}},\"timestamp\":\"2026-09-10T01:05:00.000Z\"}\n",
        )
        .unwrap();
        let rows = parse_claude_file(
            &std::fs::read_to_string(&f).unwrap(),
            &f.to_string_lossy(),
            &mut 0,
        );
        let (n3, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n3, 2);
        let c3: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c3, 2);
    }

    #[test]
    fn import_skips_gateway_dupes_and_prices() {
        let (_d, conn) = mem_db();
        // 先有一筆網關日誌
        conn.execute(
            "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, cost_usd, status, source)
             VALUES (1000000, 'codex', 'deepseek-v4-flash', 'deepseek-v4-flash', 100, 20, 0.01, 200, 'gateway')",
            [],
        )
        .unwrap();
        // 同 token 同模型同時間 → 判重跳過；另一筆不同 token → 寫入並按種子價計費
        let rows = vec![
            ParsedRow {
                app: "codex",
                model_raw: "deepseek-v4-flash".into(),
                provider_id: None,
                in_tok: 100,
                out_tok: 20,
                cache_read: 0,
                cache_write: 0,
                ts_ms: 1000050,
                import_path: "r.jsonl".into(),
            },
            ParsedRow {
                app: "codex",
                model_raw: "deepseek-v4-flash".into(),
                provider_id: None,
                in_tok: 200,
                out_tok: 30,
                cache_read: 0,
                cache_write: 0,
                ts_ms: 2000000,
                import_path: "r.jsonl".into(),
            },
        ];
        let (n, _, _, cost, dupes, _) = commit_rows(
            &conn,
            "codex",
            "r.jsonl",
            (1, 2),
            &rows,
            &["r.jsonl".to_string()],
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(dupes, 1);
        // 200*0.14 + 30*0.28 = 36.4 / 1e6
        assert!((cost - 0.0000364).abs() < 1e-9, "cost={cost}");
        let src: String = conn
            .query_row(
                "SELECT source FROM request_logs WHERE import_path='r.jsonl'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src, "import");
    }

    #[test]
    fn opencode_session_parse_maps_provider() {
        let dir = tempfile::tempdir().unwrap();
        let oc = dir.path().join("oc.db");
        {
            let c = Connection::open(&oc).unwrap();
            c.execute_batch(
                "CREATE TABLE session (id TEXT, model TEXT, time_created INTEGER, time_updated INTEGER,
                 tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER,
                 tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL);",
            )
            .unwrap();
            c.execute(
                "INSERT INTO session VALUES ('s1', '{\"id\":\"deepseek-v4-flash\",\"providerID\":\"nvidia\"}', 1000, 2000, 50, 5, 2, 0, 0, 0.0)",
                [],
            )
            .unwrap();
            c.execute("INSERT INTO session VALUES ('s2', '', 1000, 2000, 9, 9, 0, 0, 0, 0.0)", [])
                .unwrap();
        }
        let (_dg, gw) = mem_db();
        // nvidia 種子渠道 id
        let pid: i64 = gw
            .query_row("SELECT id FROM providers WHERE name='nvidia'", [], |r| r.get(0))
            .unwrap();
        let oc_conn =
            Connection::open_with_flags(&oc, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let pmap = provider_name_map(&gw);
        let mut bad = 0;
        let rows = parse_opencode_db(&oc_conn, &pmap, &mut bad);
        assert_eq!(rows.len(), 1, "空模型應跳過");
        assert_eq!(rows[0].model_raw, "deepseek-v4-flash");
        assert_eq!(rows[0].provider_id, Some(pid));
        assert_eq!(rows[0].out_tok, 7, "reasoning 併入輸出");
        assert_eq!(rows[0].import_path, "opencode:s1");
    }

    #[test]
    fn gc_orphans_removes_stale_paths() {
        let (_d, conn) = mem_db();
        for p in ["keep.jsonl", "gone.jsonl"] {
            conn.execute(
                "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, source, import_path)
                 VALUES (1000, 'codex', 'm', 'm', 1, 1, 'import', ?1)",
                [p],
            )
            .unwrap();
        }
        // 網關行不受 GC 影響
        conn.execute(
            "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, source, import_path)
             VALUES (1000, 'codex', 'm', 'm', 1, 1, 'gateway', 'gone.jsonl')",
            [],
        )
        .unwrap();
        let keep: std::collections::HashSet<String> = ["keep.jsonl".to_string()].into_iter().collect();
        let n = gc_orphans(&conn, "codex", &keep).unwrap();
        assert_eq!(n, 1);
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c, 2);
    }

    #[test]
    fn scan_lists_tools() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".claude").join("projects").join("p")).unwrap();
        std::fs::write(
            home.path().join(".claude").join("projects").join("p").join("a.jsonl"),
            "{}\n",
        )
        .unwrap();
        let tools = scan_history(home.path());
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].tool, "claude");
        assert_eq!(tools[0].files, 1);
        assert_eq!(tools[2].tool, "opencode");
        assert_eq!(tools[2].files, 0);
    }
}
