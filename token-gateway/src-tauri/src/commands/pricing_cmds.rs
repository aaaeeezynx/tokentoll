//! 全域與渠道定價命令（原 `commands.rs` 的定價段）。

use tauri::State;

use crate::db::DbState;
use crate::usage;

#[tauri::command]
pub fn pricing_list(db: State<DbState>) -> Result<Vec<usage::Pricing>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pricing_upsert(
    db: State<DbState>,
    input: usage::PricingInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_upsert(&conn, &input)
}

#[tauri::command]
pub fn pricing_delete(db: State<DbState>, model_norm: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pricing_delete(&conn, &model_norm)
}

/// 清空全域預設定價（種子有版本旗標，不會在重啟後復活）。
#[tauri::command]
pub fn pricing_clear(db: State<DbState>) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM pricing", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn pp_list(
    db: State<DbState>,
    provider_id: Option<i64>,
) -> Result<Vec<usage::ProviderPricing>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let now = crate::fsutil::now_ms();
    usage::pp_list(&conn, provider_id, now).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pp_upsert(
    db: State<DbState>,
    input: usage::ProviderPricingInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_upsert(&conn, &input)
}

#[tauri::command]
pub fn pp_delete(
    db: State<DbState>,
    provider_id: i64,
    model_norm: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_delete(&conn, provider_id, &model_norm)
}

#[tauri::command]
pub fn pp_periods(
    db: State<DbState>,
    provider_id: i64,
    model_norm: String,
) -> Result<Vec<usage::PricingPeriod>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_periods(&conn, provider_id, &model_norm).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pp_period_add(
    db: State<DbState>,
    input: usage::PricingPeriodInput,
) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_add(&conn, &input)
}

#[tauri::command]
pub fn pp_period_update(
    db: State<DbState>,
    id: i64,
    input: usage::PricingPeriodInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_update(&conn, id, &input)
}

#[tauri::command]
pub fn pp_period_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage::pp_period_delete(&conn, id)
}
