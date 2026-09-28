//! 寫入、判重與定價判定（原 `history.rs` 的「寫入」段）。

use rusqlite::{params, Connection};
use std::collections::HashMap;

use super::{DUPE_WINDOW_MS, IMPORT_SOURCE, ParsedRow};
use crate::fsutil::now_ms;
use crate::proxy::normalize_model;
use crate::usage::{rate_cost, resolve_rates};

// ---------------------------------------------------------------- 寫入 ---

pub(super) fn provider_name_map(conn: &Connection) -> HashMap<String, i64> {
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

pub(super) fn state_fingerprint(conn: &Connection, tool: &str, path: &str) -> Option<(i64, i64)> {
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
pub(super) fn gc_orphans(conn: &Connection, app: &str, keep: &std::collections::HashSet<String>) -> Result<i64, String> {
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
pub(super) fn commit_rows(
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
