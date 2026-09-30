//! Deep Link 命令（P4.1）。
//!
//! 前端拿到連結後（啟動參數或 `deeplink` 事件）呼叫 `deeplink_preview` 顯示預覽，
//! 使用者確認才呼叫 `deeplink_apply`。

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::deeplink::{self, Applied, DeeplinkPreview};

#[allow(unused_imports)]
use tauri::Emitter;

fn app_data(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// 取走啟動時帶進來的連結（前端掛載時呼叫一次）。
#[tauri::command]
pub fn deeplink_take_pending(
    state: tauri::State<'_, crate::deeplink::DeeplinkState>,
) -> Option<String> {
    let mut guard = state.0.lock().ok()?;
    guard.take()
}

/// 解析連結並產生預覽（**不寫入任何東西**）。
#[tauri::command]
pub fn deeplink_preview(url: String) -> Result<DeeplinkPreview, String> {
    let req = deeplink::parse(&url)?;
    Ok(deeplink::preview(&req))
}

/// 確認後套用。
#[tauri::command]
pub fn deeplink_apply(
    app: AppHandle,
    db: State<DbState>,
    url: String,
) -> Result<Applied, String> {
    let req = deeplink::parse(&url)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    deeplink::apply(&conn, &app_data(&app), &req)
}

/// 目前的協定註冊狀態（設定頁顯示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProtocolState {
    pub scheme: String,
    pub registered: bool,
    pub command: String,
}

#[tauri::command]
pub fn deeplink_protocol_state() -> ProtocolState {
    let command = deeplink::protocol::registered_command().unwrap_or_default();
    ProtocolState {
        scheme: deeplink::SCHEME.to_string(),
        registered: !command.is_empty(),
        command,
    }
}

/// 重新註冊（或第一次註冊）目前這個執行檔。
#[tauri::command]
pub fn deeplink_register(app: AppHandle) -> Result<ProtocolState, String> {
    let exe = std::env::current_exe().map_err(|e| format!("找不到執行檔路徑：{e}"))?;
    deeplink::protocol::ensure_registered(&exe)?;
    let _ = app; // 之後若要通知前端再擴充
    Ok(deeplink_protocol_state())
}

/// 取消註冊。
#[tauri::command]
pub fn deeplink_unregister() -> Result<ProtocolState, String> {
    deeplink::protocol::unregister()?;
    Ok(deeplink_protocol_state())
}
