//! 雲端同步命令（P4.10）。

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::cloudsync::{self, CloudState, RemoteSnapshot, SyncOutcome, WebdavConfig};
use crate::db::DbState;

fn app_data(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

fn db_path(app: &AppHandle) -> PathBuf {
    app_data(app).join("app.db")
}

#[tauri::command]
pub fn cloud_state(app: AppHandle, db: State<DbState>) -> Result<CloudState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(cloudsync::store::state(&conn, &db_path(&app)))
}

/// 寫入 WebDAV 設定（密碼留空＝保留原本的）。
#[tauri::command]
pub fn cloud_set_config(
    app: AppHandle,
    db: State<DbState>,
    base_url: String,
    username: String,
    password: String,
    remote_root: String,
    profile: String,
) -> Result<CloudState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    cloudsync::store::save(&conn, &base_url, &username, &password, &remote_root, &profile)?;
    Ok(cloudsync::store::state(&conn, &db_path(&app)))
}

/// 開啟／關閉自動同步。
#[tauri::command]
pub fn cloud_set_auto(app: AppHandle, db: State<DbState>, enabled: bool) -> Result<CloudState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::db::set_setting(&conn, "cloud_auto_sync", if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    Ok(cloudsync::store::state(&conn, &db_path(&app)))
}

/// 測試連線（唯讀，不會改動遠端或本地）。
#[tauri::command]
pub async fn cloud_test(db: State<'_, DbState>) -> Result<String, String> {
    let cfg = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        cloudsync::store::load(&conn)
    };
    if !cfg.is_configured() {
        return Err("請先填伺服器網址與帳號".to_string());
    }
    cloudsync::test_connection(&cfg).await
}

/// 先看遠端快照（下載前顯示，不動本地）。
#[tauri::command]
pub async fn cloud_remote_info(db: State<'_, DbState>) -> Result<RemoteSnapshot, String> {
    let cfg = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        cloudsync::store::load(&conn)
    };
    cloudsync::webdav::remote_snapshot(&cfg).await
}

/// 上傳（本地 → 遠端，會覆蓋遠端）。
#[tauri::command]
pub async fn cloud_upload(app: AppHandle, db: State<'_, DbState>) -> Result<SyncOutcome, String> {
    let (cfg, schema, version) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let schema: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap_or(0);
        (
            cloudsync::store::load(&conn),
            schema,
            env!("CARGO_PKG_VERSION").to_string(),
        )
    };
    let path = db_path(&app);
    let out = cloudsync::upload(&cfg, &path, schema, &version).await?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let (mtime, bytes) = cloudsync::store::local_stat(&path);
    cloudsync::store::record_uploaded_state(&conn, mtime, bytes)?;
    cloudsync::store::record_sync(&conn, true, &out.note)?;
    Ok(out)
}

/// 下載（遠端 → 本地；下載前一定先做本地安全備份）。
#[tauri::command]
pub async fn cloud_download(
    app: AppHandle,
    db: State<'_, DbState>,
) -> Result<SyncOutcome, String> {
    let app_data = app_data(&app);
    // ① 先讀設定（鎖要放掉才能 await）
    let cfg: WebdavConfig = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        cloudsync::store::load(&conn)
    };
    // ② 無鎖：抓遠端快照與資料庫位元組
    let fetched = cloudsync::fetch_remote_db(&cfg).await;
    let (snapshot, bytes) = match fetched {
        Ok(v) => v,
        Err(e) => {
            if let Ok(conn) = db.conn.lock() {
                let _ = cloudsync::store::record_sync(&conn, false, &e);
            }
            return Err(e);
        }
    };
    // ③ 上鎖：安全備份 ＋ 寫回現有連線（同步）
    let result = {
        let mut conn = db.conn.lock().map_err(|e| e.to_string())?;
        cloudsync::apply_remote_db(&mut conn, &app_data, &snapshot, &bytes)
    };
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    match result {
        Ok(out) => {
            cloudsync::store::record_sync(&conn, true, &out.note)?;
            Ok(out)
        }
        Err(e) => {
            let _ = cloudsync::store::record_sync(&conn, false, &e);
            Err(e)
        }
    }
}
