//! MCP 管理命令（P3.1）。

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::mcp::{
    self, ImportReport, McpInput, McpPreset, McpServer, SyncOutcome,
};

fn app_data(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// 全部伺服器（含逐工具啟用狀態）。
#[tauri::command]
pub fn mcp_list(db: State<DbState>) -> Result<Vec<McpServer>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    mcp::list_servers(&conn)
}

/// 內建樣板。
#[tauri::command]
pub fn mcp_presets() -> Vec<McpPreset> {
    mcp::presets()
}

/// 新增或更新伺服器；**存完立刻同步**（cc-switch：改完就寫進已啟用的工具設定檔）。
#[tauri::command]
pub fn mcp_upsert(
    app: AppHandle,
    db: State<DbState>,
    input: McpInput,
) -> Result<McpServer, String> {
    let validated = mcp::validate_input(&input)?;
    let (saved, outcomes) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let saved = mcp::upsert_server(&conn, &validated)?;
        let outcomes = mcp::sync_all(&conn, &app_data(&app))?;
        (saved, outcomes)
    };
    // 同步若失敗，把原因放到訊息裡（前端會顯示在列表上）——但存檔本身已成功，
    // 所以不當成錯誤（伺服器設定留住，使用者可以按「重新同步」再試）。
    let failed: Vec<String> = outcomes
        .iter()
        .filter(|o| !o.skipped && o.message.starts_with("同步失敗"))
        .map(|o| format!("{}：{}", o.display, o.message))
        .collect();
    if !failed.is_empty() {
        eprintln!("mcp: 同步部分失敗 {}", failed.join("；"));
    }
    Ok(saved)
}

/// 刪除伺服器（先從設定檔移除，再刪資料庫那一列）。
#[tauri::command]
pub fn mcp_delete(app: AppHandle, db: State<DbState>, id: i64) -> Result<Vec<SyncOutcome>, String> {
    let outcomes = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        mcp::delete_server(&conn, id)?;
        // 刪掉之後它就不在 managed 清單裡了，所以移除動作要跟著「少一個 slug」，
        // sync_all 會把它從各工具設定檔清掉。
        mcp::sync_all(&conn, &app_data(&app))?
    };
    Ok(outcomes)
}

/// 設定某個伺服器在某個工具上的啟用狀態（改完立刻同步那個工具）。
#[tauri::command]
pub fn mcp_set_binding(
    app: AppHandle,
    db: State<DbState>,
    id: i64,
    target: String,
    enabled: bool,
) -> Result<Vec<SyncOutcome>, String> {
    let outcomes = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        mcp::set_binding(&conn, id, &target, enabled)?;
        mcp::sync_all(&conn, &app_data(&app))?
    };
    Ok(outcomes)
}

/// 一鍵把某個工具的所有伺服器開或關（cc-switch 的整欄切換）。
#[tauri::command]
pub fn mcp_set_app_all(
    app: AppHandle,
    db: State<DbState>,
    target: String,
    enabled: bool,
) -> Result<Vec<SyncOutcome>, String> {
    let outcomes = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        for s in mcp::list_servers(&conn)? {
            mcp::set_binding(&conn, s.id, &target, enabled)?;
        }
        mcp::sync_all(&conn, &app_data(&app))?
    };
    Ok(outcomes)
}

/// 重新同步全部（不改變任何設定，只把現況寫進設定檔）。
#[tauri::command]
pub fn mcp_sync(app: AppHandle, db: State<DbState>) -> Result<Vec<SyncOutcome>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    mcp::sync_all(&conn, &app_data(&app))
}

/// 匯入既有設定（讀遍三個工具的設定檔）。
#[tauri::command]
pub fn mcp_import(app: AppHandle, db: State<DbState>) -> Result<ImportReport, String> {
    let mut rep = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        mcp::import_existing(&conn)?
    };
    // 匯入後同步一次，讓「自動對來源工具啟用」真的落到設定檔
    // （通常檔案本來就有，所以是 no-op；但若使用者剛刪過就會補回來）
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    for o in mcp::sync_all(&conn, &app_data(&app))? {
        if !o.skipped && o.message.starts_with("同步失敗") {
            rep.per_app.push(crate::mcp::sync_error(&o));
        }
    }
    Ok(rep)
}
