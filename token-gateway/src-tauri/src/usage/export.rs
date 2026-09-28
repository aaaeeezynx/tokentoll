//! 請求明細 CSV 匯出。
//!
//! 由 `usage.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;

use super::{where_sql, UsageFilter};

fn csv_esc(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 按過濾條件匯出請求明細 CSV（UTF-8，時間為本地時區）。
pub(crate) fn export_csv(conn: &Connection, f: &UsageFilter) -> rusqlite::Result<String> {
    let (w, args) = where_sql(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT ts, COALESCE(app,'unknown'), COALESCE(model_raw,''), in_tok, out_tok,
         cache_read, cache_write, (in_tok+out_tok+cache_read+cache_write),
         cost_usd, status, COALESCE(source,'')
         FROM request_logs WHERE {w} ORDER BY ts ASC"
    ))?;
    let mut out = String::from(
        "time,app,model,in_tok,out_tok,cache_read,cache_write,total_tokens,cost_usd,status,source\n",
    );
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, f64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, String>(10)?,
        ))
    })?;
    for r in rows {
        let (ts, app, model, itok, otok, cr, cw, total, cost, status, source) = r?;
        let time = chrono::DateTime::from_timestamp_millis(ts)
            .map(|u| {
                u.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{:.4},{},{}\n",
            time,
            csv_esc(&app),
            csv_esc(&model),
            itok,
            otok,
            cr,
            cw,
            total,
            cost,
            status,
            csv_esc(&source)
        ));
    }
    Ok(out)
}
