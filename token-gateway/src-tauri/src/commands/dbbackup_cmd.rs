//! 資料庫備份管理命令（P4.8）。

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::dbbackup::{self, BackupInfo, BackupState, RestoreOutcome};

fn app_data(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

#[tauri::command]
pub fn db_backup_state(app: AppHandle, db: State<DbState>) -> Result<BackupState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(dbbackup::state(&conn, &app_data(&app)))
}

/// 立即備份（可指定名稱；不給就用時間戳）。
#[tauri::command]
pub fn db_backup_now(
    app: AppHandle,
    db: State<DbState>,
    name: Option<String>,
) -> Result<BackupInfo, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    dbbackup::create(&conn, &app_data(&app), name)
}

/// 還原（會先自動備份當前資料庫）。
#[tauri::command]
pub fn db_backup_restore(
    app: AppHandle,
    db: State<DbState>,
    name: String,
) -> Result<RestoreOutcome, String> {
    let mut conn = db.conn.lock().map_err(|e| e.to_string())?;
    dbbackup::restore(&mut conn, &app_data(&app), &name)
}

/// 改名。
#[tauri::command]
pub fn db_backup_rename(
    app: AppHandle,
    name: String,
    new_name: String,
) -> Result<BackupInfo, String> {
    dbbackup::rename(&app_data(&app), &name, &new_name)
}

/// 刪除。
#[tauri::command]
pub fn db_backup_delete(app: AppHandle, name: String) -> Result<(), String> {
    dbbackup::delete(&app_data(&app), &name)
}

/// 設定自動備份間隔（小時；0＝停用）與保留份數。
#[tauri::command]
pub fn db_backup_set_schedule(
    app: AppHandle,
    db: State<DbState>,
    interval_hours: Option<i64>,
    retention: Option<i64>,
) -> Result<BackupState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    if let Some(h) = interval_hours {
        if !dbbackup::INTERVAL_OPTIONS.contains(&h) {
            return Err(format!("不支援的間隔：{h} 小時"));
        }
        crate::db::set_setting(&conn, "db_backup_interval", &h.to_string())
            .map_err(|e| e.to_string())?;
    }
    if let Some(k) = retention {
        if !dbbackup::RETENTION_OPTIONS.contains(&k) {
            return Err(format!("不支援的保留份數：{k}"));
        }
        crate::db::set_setting(&conn, "db_backup_keep", &k.to_string())
            .map_err(|e| e.to_string())?;
    }
    Ok(dbbackup::state(&conn, &app_data(&app)))
}
