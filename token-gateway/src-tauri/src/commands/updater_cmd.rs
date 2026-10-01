//! 自動更新命令（P4.9）：設定 manifest 網址、檢查更新、看上次結果。

use serde::Serialize;
use tauri::State;

use crate::db::DbState;
use crate::updater::{self, UpdateCheck};

#[derive(Debug, Clone, Serialize)]
pub struct UpdateState {
    /// 目前版本
    pub current: String,
    /// 設定的 manifest 網址（留空＝不檢查）
    pub manifest_url: String,
    pub last_check_ms: i64,
    pub last_result: String,
    /// 我們**不做自動安裝**（需要簽章更新檔 ＋ updater 外掛），這段給 UI 說明
    pub auto_install: bool,
}

#[tauri::command]
pub fn update_state(db: State<DbState>) -> Result<UpdateState, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let (last_check_ms, last_result) = updater::check::last_result(&conn);
    Ok(UpdateState {
        current: updater::check::current_version(),
        manifest_url: updater::check::url_setting(&conn),
        last_check_ms,
        last_result,
        auto_install: false,
    })
}

/// 設定 manifest 網址。
#[tauri::command]
pub fn update_set_url(db: State<DbState>, url: String) -> Result<UpdateState, String> {
    let u = url.trim().to_string();
    if !u.is_empty() && !u.starts_with("http://") && !u.starts_with("https://") {
        return Err(format!("網址必須是 http(s)：{url}"));
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    crate::db::set_setting(&conn, updater::URL_KEY, &u).map_err(|e| e.to_string())?;
    drop(conn);
    update_state(db)
}

/// 檢查更新（抓 manifest、比版本）。
#[tauri::command]
pub async fn update_check(db: State<'_, DbState>) -> Result<UpdateCheck, String> {
    let (url, saved) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        (updater::check::url_setting(&conn), updater::check::current_version())
    };
    if url.is_empty() {
        return Err("請先填更新資訊（manifest）網址".to_string());
    }
    let text = reqwest::get(&url)
        .await
        .map_err(|e| format!("抓取更新資訊失敗：{e}"))?
        .error_for_status()
        .map_err(|e| format!("更新資訊伺服器回錯：{e}"))?
        .text()
        .await
        .map_err(|e| format!("讀取更新資訊失敗：{e}"))?;
    let result = updater::check_text(&text)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    updater::check::record(&conn, &format!("{}（{}）", result.message, saved))?;
    Ok(result)
}
