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
/// v10（2026-10-01）：新增 provider_usage_query（每來源的用量查詢設定；
/// P2.1 對標 cc-switch 的 Usage Query，同樣是純新增表）。
/// v11（2026-10-01）：新增 mcp_servers / mcp_bindings（P3.1 MCP 管理，
/// 同樣是純新增表）。
/// v12（2026-10-01）：新增 prompt_presets（P3.2 提示詞預設集，純新增表）。
/// v13（2026-10-01）：新增 skill_repos / skills / skill_bindings（P3.3 技能管理）。
/// v14（2026-10-02）：**移除** MCP／提示詞／技能三個功能，連帶 DROP 掉
/// mcp_servers / mcp_bindings / prompt_presets / skill_repos / skills /
/// skill_bindings 六張表。使用者已經寫進各工具設定檔的內容（`AGENTS.md`、
/// `config.toml` 的 `[mcp_servers]`、`skills/` 目錄）**一律不動** —— 移除的是
/// 這個 App 的管理能力，不是使用者的檔案。
/// v15（2026-10-02）：**移除** Deep Link 匯入／資料庫備份／更新檢查／雲端同步
/// 四個功能。與 v14 不同 —— 這四個功能都沒有專屬資料表，狀態一律放在通用的
/// `settings` key-value 表裡，所以不需要 DROP TABLE，只需要把只服務它們的
/// 17 個 key 清掉（見 [`open::purge_removed_feature_settings`]`）。
/// 當中 `cloud_password` 是**明文的 WebDAV 密碼**，留著就是留著憑證在磁碟上。
pub const SCHEMA_VERSION: i32 = 15;

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
