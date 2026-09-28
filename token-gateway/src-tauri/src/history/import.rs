//! 回填入口（原 `history.rs` 的「入口」段）。

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashMap;

use super::commit::{commit_rows, gc_orphans, provider_name_map, state_fingerprint};
use super::parse::{parse_claude_file, parse_codex_file, parse_opencode_db};
use super::{collect_jsonl_files, file_fp};

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
