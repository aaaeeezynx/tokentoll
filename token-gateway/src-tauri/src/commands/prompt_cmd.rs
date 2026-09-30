//! 提示詞預設集命令（P3.2）。

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::prompt::{
    self, BackfillOutcome, PanelState, PromptInput, PromptPreset,
};

fn app_data(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// 面板狀態：檔案路徑／大小、啟用中的預設集、檔案是否被手改過。
///
/// 順便做**首次啟動匯入**（cc-switch 也有這個行為）：該工具一列預設集都沒有、
/// 而檔案已有內容時，把內容收成一個預設集並啟用 —— 這樣使用者第一次打開面板時
/// 原本的 `AGENTS.md` 不會因為之後的切換而消失。
#[tauri::command]
pub fn prompt_state(db: State<DbState>, app: String) -> Result<PanelState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let imported = prompt::ensure_imported(&conn, &app)?;
    let mut st = prompt::panel_state(&conn, &app)?;
    if let Some(p) = imported {
        st.active_id = Some(p.id);
        st.active_name = p.name;
        st.dirty = false;
    }
    Ok(st)
}

/// 全部支援的工具與各自的檔案路徑（前端列頁籤用）。
#[tauri::command]
pub fn prompt_apps() -> Vec<PromptApp> {
    prompt::supported_apps()
        .into_iter()
        .map(|app| {
            let display = crate::tools::APPS
                .iter()
                .find(|(a, _)| *a == app)
                .map(|(_, d)| *d)
                .unwrap_or(app.as_str())
                .to_string();
            let path = prompt::path_for(&app)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            PromptApp {
                installed: prompt::app_installed(&app),
                app,
                display,
                path,
            }
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptApp {
    pub app: String,
    pub display: String,
    pub path: String,
    pub installed: bool,
}

/// 某個工具的全部預設集。
#[tauri::command]
pub fn prompt_list(db: State<DbState>, app: String) -> Result<Vec<PromptPreset>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    prompt::list_presets(&conn, &app)
}

/// 新增／編輯。
///
/// 編輯**啟用中的**那一個時，存完直接把新內容寫進檔案，**不做回填** ——
/// 使用者的編輯就是意圖；這時候回填會把他剛寫的內容換成舊檔案內容
/// （這個順序問題是寫測試時抓到的）。
#[tauri::command]
pub fn prompt_save(
    app: AppHandle,
    db: State<DbState>,
    input: PromptInput,
) -> Result<PromptPreset, String> {
    let clean = input.validated()?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let saved = prompt::save_preset(&conn, &clean)?;
    if saved.active {
        prompt::write_active(&conn, &app_data(&app), &saved.app)?;
    }
    Ok(saved)
}

/// 啟用某個預設集：**先把現行檔案回填進舊的，再切換並寫入新的**。
#[tauri::command]
pub fn prompt_activate(
    app: AppHandle,
    db: State<DbState>,
    id: i64,
) -> Result<BackfillOutcome, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let target = prompt::get_preset(&conn, id)?;
    prompt::activate_and_sync(&conn, &app_data(&app), &target.app, target.id)
}

/// 關掉某個工具啟用中的預設集（**檔案內容留著**，只是不再由我們管理）。
#[tauri::command]
pub fn prompt_deactivate(
    app: AppHandle,
    db: State<DbState>,
    target: String,
) -> Result<(), String> {
    let _ = &app;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::prompt::deactivate_active(&conn, &target)
}

/// 刪除預設集（啟用中的會被拒）。
#[tauri::command]
pub fn prompt_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    prompt::delete_preset(&conn, id)
}

/// 重新同步（不改變任何設定，只跑一次「回填 → 寫檔」）。
#[tauri::command]
pub fn prompt_sync(
    app: AppHandle,
    db: State<DbState>,
    target: String,
) -> Result<BackfillOutcome, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    prompt::sync_app(&conn, &app_data(&app), &target)
}

/// 讀目前檔案內容（UI 的「看檔案」用；不會寫任何東西）。
#[tauri::command]
pub fn prompt_live(target: String) -> Result<String, String> {
    prompt::live_content(&target)
}
