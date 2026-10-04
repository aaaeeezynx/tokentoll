//! 匯出：把來源組成可攜的 `ExportFile`
//!
//! （從 `providers_io.rs` 拆出來，讓那支檔案維持在 400 行以內。）

use super::*;

/// 匯出全部來源。`include_keys=false` 時 `api_key` 一律空字串。
pub(crate) fn export_providers(
    conn: &Connection,
    include_keys: bool,
) -> Result<ExportFile, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, app_type, api_format, base_url, api_key, auth_scheme,
                    models_json, enabled
             FROM providers ORDER BY priority ASC, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, ExportProvider)> = stmt
        .query_map([], |r| {
            let models_json: String = r.get(7)?;
            Ok((
                r.get::<_, i64>(0)?,
                ExportProvider {
                    name: r.get(1)?,
                    app_type: r.get(2)?,
                    api_format: r.get(3)?,
                    base_url: r.get(4)?,
                    api_key: if include_keys { r.get(5)? } else { String::new() },
                    auth_scheme: r.get(6)?,
                    models_json: serde_json::from_str(&models_json).unwrap_or_default(),
                    enabled: r.get::<_, i64>(8)? != 0,
                    models: Vec::new(),
                    pricing: Vec::new(),
                    periods: Vec::new(),
                },
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::with_capacity(rows.len());
    for (id, mut p) in rows {
        p.models = collect_models(conn, id)?;
        p.pricing = collect_pricing(conn, id)?;
        p.periods = collect_periods(conn, id)?;
        out.push(p);
    }
    Ok(ExportFile {
        kind: EXPORT_KIND.to_string(),
        version: EXPORT_VERSION,
        exported_at: now_ms(),
        includes_keys: include_keys,
        providers: out,
    })
}

fn collect_models(conn: &Connection, pid: i64) -> Result<Vec<ExportModel>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT display_name, actual_model, context_window, reasoning, ord, enabled
             FROM provider_models WHERE provider_id=?1 ORDER BY ord ASC, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let v = stmt
        .query_map([pid], |r| {
            Ok(ExportModel {
                display_name: r.get(0)?,
                actual_model: r.get(1)?,
                context_window: r.get(2)?,
                reasoning: r.get(3)?,
                ord: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(v)
}

fn collect_pricing(conn: &Connection, pid: i64) -> Result<Vec<ExportPricing>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT model_norm, mode, in_pm, out_pm, cache_read_pm, cache_create_pm,
                    sub_fee_usd, sub_included_tokens, source
             FROM provider_pricing WHERE provider_id=?1 ORDER BY model_norm ASC",
        )
        .map_err(|e| e.to_string())?;
    let v = stmt
        .query_map([pid], |r| {
            Ok(ExportPricing {
                model_norm: r.get(0)?,
                mode: r.get(1)?,
                in_pm: r.get(2)?,
                out_pm: r.get(3)?,
                cache_read_pm: r.get(4)?,
                cache_create_pm: r.get(5)?,
                sub_fee_usd: r.get(6)?,
                sub_included_tokens: r.get(7)?,
                source: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(v)
}

fn collect_periods(conn: &Connection, pid: i64) -> Result<Vec<ExportPeriod>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT model_norm, name, start, end, ord, in_pm, out_pm,
                    cache_read_pm, cache_create_pm
             FROM pricing_periods WHERE provider_id=?1 ORDER BY ord ASC, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let v = stmt
        .query_map([pid], |r| {
            Ok(ExportPeriod {
                model_norm: r.get(0)?,
                name: r.get(1)?,
                start: r.get(2)?,
                end: r.get(3)?,
                ord: r.get(4)?,
                in_pm: r.get(5)?,
                out_pm: r.get(6)?,
                cache_read_pm: r.get(7)?,
                cache_create_pm: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(v)
}
