//! 各來源格式解析（原 `history.rs` 的「解析」段）。

use rusqlite::Connection;
use std::collections::HashMap;

use super::{num, parse_ts, str_field, ParsedRow};

// ---------------------------------------------------------------- 解析 ---

/// Claude Code：assistant 行的 message.usage。
pub(super) fn parse_claude_file(text: &str, import_path: &str, bad: &mut i64) -> Vec<ParsedRow> {
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
pub(super) fn parse_codex_file(text: &str, import_path: &str, bad: &mut i64) -> Vec<ParsedRow> {
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

/// DeepSeek Harness：一會話一列，取自專案快取的用量投影
/// `record.rows.tokenUsage.val.totals`（與 OpenCode 同口徑：按會話計）。
///
/// **口徑**：DSH 把 prompt 拆成 `uncachedInputTokens`（沒命中快取的部分）與
/// `cacheReadTokens`（命中快取的部分），兩者互斥；所以總輸入是兩者相加 ——
/// 這樣才與 Codex／Claude／網關的 `input_tokens`（含快取）同一個意思，
/// `cache_read` 則是其中的子集。
///
/// **時間**：投影檔裡只有 `identity.createdAt`（會話開始）與 `seq`，沒有每次呼叫
/// 的時間；所以用**檔案最後更新時間**（＝會話最後活動）當這一列的時間，與
/// OpenCode 用 `time_updated` 的做法一致。呼叫端把 mtime 傳進來。
///
/// DSH 的完整逐次呼叫紀錄在 `~/.dsh/sessions/**/session.v3.jsonl.zstd`（zstd 多框架），
/// 但這份投影檔是**純 JSON** 且已含會話總量，所以不必為了用量引入 zstd 相依。
pub(super) fn parse_dsh_session(
    text: &str,
    import_path: &str,
    ts_ms: i64,
    bad: &mut i64,
) -> Vec<ParsedRow> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        *bad += 1;
        return Vec::new();
    };
    let Some(rows) = v.get("record").and_then(|r| r.get("rows")) else {
        *bad += 1;
        return Vec::new();
    };
    let Some(totals) = rows
        .get("tokenUsage")
        .and_then(|t| t.get("val"))
        .and_then(|t| t.get("totals"))
    else {
        *bad += 1;
        return Vec::new();
    };
    let cache_read = num(totals, "cacheReadTokens");
    let in_tok = num(totals, "uncachedInputTokens") + cache_read;
    let out_tok = num(totals, "outputTokens");
    let cache_write = num(totals, "cacheWriteTokens");
    if in_tok + out_tok + cache_write <= 0 {
        return Vec::new(); // 空會話不記（也不佔一列）
    }
    // 使用者可以在 DSH 裡中途換模型；投影只留最後一次，取它最能代表這個會話。
    let model_raw = rows
        .get("modelSelection")
        .and_then(|m| m.get("val"))
        .and_then(|m| m.get("lastUsed"))
        .and_then(|l| l.get("model"))
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    vec![ParsedRow {
        app: "dsh",
        model_raw: if model_raw.is_empty() {
            "(DSH 未記錄模型)".to_string()
        } else {
            model_raw
        },
        provider_id: None,
        in_tok,
        out_tok,
        cache_read,
        cache_write,
        ts_ms,
        import_path: import_path.to_string(),
    }]
}

/// OpenCode：session 表一行一會話；model 是 JSON（取 id）；providerID 回填渠道。
pub(super) fn parse_opencode_db(
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
