//! 模型目錄與價格識別命令（原 `commands.rs` 的「模型」與「價格識別」段）。

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;

// ---------------------------------------------------------------- 模型 ---

#[tauri::command]
pub async fn catalog_fetch(
    app: AppHandle,
    provider_id: i64,
    api_key: Option<String>,
    base_url: Option<String>,
) -> Result<crate::models::CatalogCache, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        // 短鎖讀配置 → 放鎖做網路（最多 30s）→ 短鎖寫快取：全程不佔主執行緒、不扣全局鎖做網路。
        let cfg = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            crate::models::catalog_read_cfg(
                &conn,
                provider_id,
                api_key.as_deref(),
                base_url.as_deref(),
            )?
        };
        let ids = crate::models::catalog_fetch_http(&cfg)?;
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        crate::models::catalog_write(&conn, provider_id, ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn catalog_get(
    db: State<DbState>,
    provider_id: i64,
) -> Result<crate::models::CatalogCache, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::catalog_get(&conn, provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn models_list(
    db: State<DbState>,
    provider_id: i64,
) -> Result<Vec<crate::models::UsableModel>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::models_list(&conn, provider_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn model_counts(db: State<DbState>) -> Result<Vec<(i64, i64)>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_counts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn model_add(
    db: State<DbState>,
    input: crate::models::UsableModelInput,
) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_add(&conn, &input)
}

#[tauri::command]
pub fn model_update(
    db: State<DbState>,
    id: i64,
    input: crate::models::UsableModelInput,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_update(&conn, id, &input)
}

#[tauri::command]
pub fn model_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::models::model_delete(&conn, id)
}

// ---------------------------------------------------------------- 價格識別 ---

/// 用已加入來源的模型抽取定價（Key 只在後端使用，不寫 request_logs）。
#[tauri::command]
pub async fn price_extract(
    app: AppHandle,
    provider_id: i64,
    model: String,
    url: Option<String>,
    pasted_text: Option<String>,
    image_base64: Option<String>,
    target_hint: Option<String>,
) -> Result<crate::price_extract::PriceExtractResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        // 短鎖讀憑證 → 放鎖做代抓/LLM 抽取（最多 90s+15s）：不持全局鎖做網路。
        let ctx = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            crate::price_extract::extract_read_ctx(&conn, provider_id, &model)?
        };
        crate::price_extract::extract_run(
            &ctx,
            url.as_deref(),
            pasted_text.as_deref(),
            image_base64.as_deref(),
            target_hint.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
