//! 請求日誌、歷史回填與應用程式資料目錄命令（原 `commands.rs` 的「日誌」與「歷史回填」段）。

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::history;
use crate::proxy;

// ---------------------------------------------------------------- 日誌 ---

#[tauri::command]
pub fn recent_logs(
    db: State<DbState>,
    limit: i64,
) -> Result<Vec<proxy::LogRow>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    proxy::recent_logs(&conn, limit).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 歷史回填 ---

/// 掃描本機會話來源（乾跑，只統計文件數/位元組/會話數，不寫庫）。
#[tauri::command]
pub fn history_scan() -> Result<Vec<history::ScanTool>, String> {
    let home = dirs::home_dir().ok_or("找不到用戶主目錄".to_string())?;
    Ok(history::scan_history(&home))
}

/// 執行回填（87MB 級解析放 blocking 執行緒，避免卡住命令執行緒）。
#[tauri::command]
pub async fn history_import(
    app: AppHandle,
    tools: Vec<String>,
) -> Result<history::ImportSummary, String> {
    for t in &tools {
        if !["claude", "codex", "opencode"].contains(&t.as_str()) {
            return Err(format!("未知工具：{t}"));
        }
    }
    tokio::task::spawn_blocking(move || {
        let st = app.state::<DbState>();
        let conn = st.conn.lock().map_err(|e| e.to_string())?;
        history::import_history(&conn, &tools)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn app_data_dir(app: AppHandle) -> Result<String, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string())
}
