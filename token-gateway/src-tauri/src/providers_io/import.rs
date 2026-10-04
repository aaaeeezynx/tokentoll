//! 匯入：skip 語意、回報新增／跳過／改名
//!
//! （從 `providers_io.rs` 拆出來，讓那支檔案維持在 400 行以內。）

use super::*;

/// 匯入來源（skip 語意：同 `(name, base_url)` 已存在就跳過）。
pub(crate) fn import_providers(
    conn: &Connection,
    file: &ExportFile,
) -> Result<ImportReport, String> {
    if !EXPORT_KINDS.contains(&file.kind.as_str()) {
        return Err(format!(
            "這不是來源匯出檔（kind={}，應為 {EXPORT_KIND}）",
            file.kind
        ));
    }
    if file.version > EXPORT_VERSION {
        return Err(format!(
            "匯出檔版本太新（{} > {EXPORT_VERSION}），請先更新 App",
            file.version
        ));
    }
    let mut rep = ImportReport {
        added: 0,
        skipped: 0,
        models: 0,
        pricing: 0,
        periods: 0,
        names: Vec::new(),
        includes_keys: file.includes_keys,
    };
    for p in &file.providers {
        let base_url = p.base_url.trim();
        if p.name.trim().is_empty() || base_url.is_empty() {
            rep.skipped += 1;
            continue;
        }
        let dup: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM providers WHERE name=?1 AND base_url=?2",
                params![p.name.trim(), base_url],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if dup > 0 {
            rep.skipped += 1;
            continue;
        }
        let name = unique_name(conn, p.name.trim(), "").map_err(|e| e.to_string())?;
        let now = now_ms();
        let next = next_priority(conn).map_err(|e| e.to_string())?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                    models_json, priority, enabled, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
            params![
                name,
                p.app_type,
                p.api_format,
                base_url,
                p.api_key,
                norm_scheme(&p.auth_scheme),
                serde_json::to_string(&p.models_json).unwrap_or_else(|_| "[]".into()),
                next,
                if p.enabled { 1 } else { 0 },
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        let pid = tx.last_insert_rowid();
        for m in &p.models {
            if m.display_name.trim().is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO provider_models (provider_id, display_name, actual_model,
                                                        context_window, reasoning, ord, enabled)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    pid,
                    m.display_name.trim(),
                    if m.actual_model.trim().is_empty() {
                        m.display_name.trim()
                    } else {
                        m.actual_model.trim()
                    },
                    m.context_window,
                    m.reasoning,
                    m.ord,
                    if m.enabled { 1 } else { 0 }
                ],
            )
            .map_err(|e| e.to_string())?;
            rep.models += 1;
        }
        for pr in &p.pricing {
            tx.execute(
                "INSERT OR REPLACE INTO provider_pricing (provider_id, model_norm, mode, in_pm,
                                                          out_pm, cache_read_pm, cache_create_pm,
                                                          sub_fee_usd, sub_included_tokens, source)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    pid,
                    pr.model_norm,
                    if pr.mode.is_empty() { "usage" } else { &pr.mode },
                    pr.in_pm,
                    pr.out_pm,
                    pr.cache_read_pm,
                    pr.cache_create_pm,
                    pr.sub_fee_usd,
                    pr.sub_included_tokens,
                    pr.source
                ],
            )
            .map_err(|e| e.to_string())?;
            rep.pricing += 1;
        }
        for pe in &p.periods {
            tx.execute(
                "INSERT INTO pricing_periods (provider_id, model_norm, name, start, end, ord,
                                              in_pm, out_pm, cache_read_pm, cache_create_pm)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    pid,
                    pe.model_norm,
                    pe.name,
                    pe.start,
                    pe.end,
                    pe.ord,
                    pe.in_pm,
                    pe.out_pm,
                    pe.cache_read_pm,
                    pe.cache_create_pm
                ],
            )
            .map_err(|e| e.to_string())?;
            rep.periods += 1;
        }
        tx.commit().map_err(|e| e.to_string())?;
        rep.added += 1;
        rep.names.push(name);
    }
    Ok(rep)
}

// ---------------------------------------------------------------- 命令 ---
