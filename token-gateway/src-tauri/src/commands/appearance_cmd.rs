//! 外觀與啟動的命令（P4.3 主題、P4.4 開機自啟）。

use rusqlite::Connection;
use tauri::State;

use crate::db::DbState;
use crate::{autostart, theme};

/// 目前的外觀設定。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppearanceState {
    /// system | light | dark
    pub theme: String,
    pub autostart: bool,
    /// 登錄檔裡實際的啟動命令（空＝沒註冊）
    pub autostart_command: String,
    pub exe: String,
}

fn theme_setting(conn: &Connection) -> String {
    crate::db::get_setting(conn, "theme")
        .map(|s| theme::validate(&s).unwrap_or_else(|_| "system".to_string()))
        .unwrap_or_else(|| "system".to_string())
}

#[tauri::command]
pub fn appearance_state(db: State<DbState>) -> Result<AppearanceState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(AppearanceState {
        theme: theme_setting(&conn),
        autostart: autostart::registered_command().is_some(),
        autostart_command: autostart::registered_command().unwrap_or_default(),
        exe: std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    })
}

/// 設定主題（system／light／dark）。
#[tauri::command]
pub fn appearance_set_theme(db: State<DbState>, value: String) -> Result<String, String> {
    let t = theme::validate(&value)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::db::set_setting(&conn, "theme", &t).map_err(|e| e.to_string())?;
    Ok(t)
}

/// 打開／關閉開機自啟。
#[tauri::command]
pub fn appearance_set_autostart(enabled: bool) -> Result<AppearanceState, String> {
    let exe = std::env::current_exe().map_err(|e| format!("找不到執行檔路徑：{e}"))?;
    autostart::set_enabled(enabled, &exe)?;
    Ok(AppearanceState {
        theme: "system".to_string(), // 由前端覆蓋；這裡只是佔位
        autostart: autostart::registered_command().is_some(),
        autostart_command: autostart::registered_command().unwrap_or_default(),
        exe: exe.to_string_lossy().to_string(),
    })
}
