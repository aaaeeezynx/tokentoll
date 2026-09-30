//! 會話管理命令（P3.4）。
//!
//! 掃描與讀取是**唯讀**（可以慢慢做），刪除走 `SessionEnv` 的安全界線。
//! 掃描會讀很多檔案，放進 `spawn_blocking` 避免卡住 UI 執行緒。

use serde::Serialize;
use tauri::State;

use crate::db::DbState;
use crate::sessions::{
    self, DeleteOutcome, SessionMessage, SessionSummary,
};

/// 可用的工具（前端畫篩選器）。
#[tauri::command]
pub fn sessions_apps() -> Vec<SessionApp> {
    sessions::supported_apps()
        .into_iter()
        .map(|app| {
            let display = crate::tools::APPS
                .iter()
                .find(|(a, _)| *a == app)
                .map(|(_, d)| *d)
                .unwrap_or(app)
                .to_string();
            SessionApp {
                app: app.to_string(),
                display,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionApp {
    pub app: String,
    pub display: String,
}

/// 掃描會話（可指定工具；空＝全部）。
#[tauri::command]
pub async fn sessions_scan(apps: Option<Vec<String>>) -> Result<Vec<SessionSummary>, String> {
    let targets = apps.unwrap_or_else(|| {
        sessions::supported_apps()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    });
    tauri::async_runtime::spawn_blocking(move || sessions::scan_all(&targets))
        .await
        .map_err(|e| e.to_string())
}

/// 讀一則會話的對話內容。
#[tauri::command]
pub async fn sessions_read(
    app: String,
    session_id: String,
    path: String,
) -> Result<Vec<SessionMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        sessions::read_transcript(&app, &session_id, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 刪除一則（或一批）會話。`targets` 是 `(app, path)`。
#[tauri::command]
pub async fn sessions_delete(
    _db: State<'_, DbState>,
    targets: Vec<(String, String)>,
) -> Result<DeleteOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let env = sessions::SessionEnv::real()?;
        Ok::<DeleteOutcome, String>(sessions::delete_sessions(&env, &targets))
    })
    .await
    .map_err(|e| e.to_string())?
}
