//! SQLite 存儲層（M1）：建表 + WAL + 單連接 Mutex。
//!
//! 資料檔位於系統應用程式數據目錄，Windows 下為
//! `%APPDATA%/com.tokencounter.gateway/app.db`。
//! 表結構已按 M2（上游渠道）/ M3（本地 Key + 請求日誌）/ M4（定價）預留。

//! 檔案佈局（純重構；對外路徑 `crate::db::*` 完全不變）：
//! - `schema`：建表 SQL（`SCHEMA`）與內置種子（`SEED_PRICING` / `SEED_PROVIDERS`）
//! - `open`：`open_and_ensure`（開啟 + 冪等遷移 + 種子）
//! - `settings`：`settings` 表存取（`get_setting` / `set_setting`）
//! - `tests`：單測（再依主題拆成子模組）

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

mod open;
mod schema;
mod settings;

#[cfg(test)]
mod tests;

pub(crate) use open::*;
pub(crate) use schema::*;
pub(crate) use settings::*;

/// 當前 schema 版本。後續升級時遞增，並補充 ALTER 遷移。
/// v8（Phase 1）：新增 provider_stripped_fields / proxy_trace（純新增表，
/// 走 SCHEMA 的 CREATE TABLE IF NOT EXISTS，無需 ALTER）。
/// v9（2026-09-30）：新增 provider_model_protocol（學到的上游協議記憶；
/// 同樣是純新增表，無需 ALTER）。
pub const SCHEMA_VERSION: i32 = 9;

/// 網關預設連接埠（C 方案：可在設定中修改；歷史預設 15721 與 cc-switch 衝突）。
pub const DEFAULT_GATEWAY_PORT: u16 = 15722;

pub struct DbState {
    pub conn: Mutex<Connection>,
    pub path: PathBuf,
}

pub fn init_db(app: &AppHandle) -> Result<DbState, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("app.db");
    let conn = open_and_ensure(&path)?;
    Ok(DbState {
        conn: Mutex::new(conn),
        path,
    })
}
