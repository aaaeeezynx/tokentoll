//! 複製來源（連模型、價格、訂閱時段一起複製）
//!
//! （從 `providers_io.rs` 拆出來，讓那支檔案維持在 400 行以內。）

use super::*;

/// 複製一個來源（連同模型、每模型價格、訂閱時段）。
pub(crate) fn duplicate_provider(conn: &Connection, id: i64) -> Result<i64, String> {
    let src = conn
        .query_row(
            "SELECT name, app_type, api_format, base_url, api_key, auth_scheme, models_json,
                    priority, enabled
             FROM providers WHERE id=?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, i64>(8)?,
                ))
            },
        )
        .map_err(|e| format!("來源不存在（{id}）：{e}"))?;
    let name = unique_name(conn, &src.0, " 複製").map_err(|e| e.to_string())?;
    let now = now_ms();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    // 插在原本那筆後面：把原位置之後的 priority 全部後移一格。
    tx.execute(
        "UPDATE providers SET priority = priority + 1 WHERE priority > ?1",
        [src.7],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                models_json, priority, enabled, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
        params![
            name,
            src.1,
            src.2,
            src.3,
            src.4,
            src.5,
            src.6,
            src.7 + 1,
            src.8,
            now
        ],
    )
    .map_err(|e| e.to_string())?;
    let new_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO provider_models (provider_id, display_name, actual_model, context_window,
                                      reasoning, ord, enabled)
         SELECT ?1, display_name, actual_model, context_window, reasoning, ord, enabled
         FROM provider_models WHERE provider_id=?2",
        params![new_id, id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO provider_pricing (provider_id, model_norm, mode, in_pm, out_pm,
                                       cache_read_pm, cache_create_pm, sub_fee_usd,
                                       sub_included_tokens, source)
         SELECT ?1, model_norm, mode, in_pm, out_pm, cache_read_pm, cache_create_pm,
                sub_fee_usd, sub_included_tokens, source
         FROM provider_pricing WHERE provider_id=?2",
        params![new_id, id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO pricing_periods (provider_id, model_norm, name, start, end, ord,
                                      in_pm, out_pm, cache_read_pm, cache_create_pm)
         SELECT ?1, model_norm, name, start, end, ord, in_pm, out_pm,
                cache_read_pm, cache_create_pm
         FROM pricing_periods WHERE provider_id=?2",
        params![new_id, id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(new_id)
}
