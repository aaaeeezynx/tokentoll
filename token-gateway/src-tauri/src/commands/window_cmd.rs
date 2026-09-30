//! 視窗行為命令（P4.7）。

use tauri::{AppHandle, State};

use crate::db::DbState;
use crate::window::{self, CloseAction};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowBehavior {
    /// tray（預設）| exit
    pub close_action: String,
    /// 現在是不是處於輕量模式（沒有視窗）
    pub lightweight: bool,
    /// 目前設定的啟動自啟（顯示用；真相在登錄檔）
    pub autostart: bool,
}

fn snapshot(app: &AppHandle, conn: &rusqlite::Connection) -> WindowBehavior {
    WindowBehavior {
        close_action: window::close_action(conn).as_str().to_string(),
        lightweight: window::is_lightweight(app),
        autostart: crate::autostart::registered_command().is_some(),
    }
}

#[tauri::command]
pub fn window_behavior(app: AppHandle, db: State<DbState>) -> Result<WindowBehavior, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(snapshot(&app, &conn))
}

/// 設定「關閉視窗時」的行為（tray＝縮到系統匣、exit＝直接結束）。
#[tauri::command]
pub fn window_set_close_action(
    app: AppHandle,
    db: State<DbState>,
    action: String,
) -> Result<WindowBehavior, String> {
    let parsed = CloseAction::parse(&action);
    if action.trim() != parsed.as_str() && !action.trim().is_empty() {
        return Err(format!("不支援的關閉行為「{action}」（可用：tray／exit）"));
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::db::set_setting(&conn, "close_action", parsed.as_str()).map_err(|e| e.to_string())?;
    Ok(snapshot(&app, &conn))
}

/// 進入輕量模式（銷毀視窗；網關繼續跑）。
#[tauri::command]
pub fn window_enter_lightweight(app: AppHandle) -> Result<(), String> {
    window::enter_lightweight(&app)
}

/// 顯示主視窗（視窗不存在就重建）。
#[tauri::command]
pub fn window_show_main(app: AppHandle) -> Result<(), String> {
    window::show_main(&app)
}
