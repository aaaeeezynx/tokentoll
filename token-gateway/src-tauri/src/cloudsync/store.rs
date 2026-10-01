//! WebDAV 的設定存取與狀態（`cloudsync` 的資料層）。

use rusqlite::Connection;
use serde::Serialize;

use super::{mask_password, should_auto_sync, DEFAULT_PROFILE, DEFAULT_ROOT};

/// WebDAV 設定（密碼在 UI 只顯示遮罩）。
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct WebdavConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub remote_root: String,
    pub profile: String,
}

impl WebdavConfig {
    pub fn is_configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.username.trim().is_empty()
    }

    /// 給 UI 的副本（密碼遮罩）。
    pub fn masked(&self) -> Self {
        Self {
            password: mask_password(&self.password),
            ..self.clone()
        }
    }
}

pub fn load(conn: &Connection) -> WebdavConfig {
    let g = |k: &str, d: &str| crate::db::get_setting(conn, k).unwrap_or_else(|| d.to_string());
    WebdavConfig {
        base_url: g("cloud_base_url", ""),
        username: g("cloud_username", ""),
        password: g("cloud_password", ""),
        remote_root: g("cloud_remote_root", DEFAULT_ROOT),
        profile: g("cloud_profile", DEFAULT_PROFILE),
    }
}

/// 寫入設定（密碼給空字串＝保留原本的，與 cc-switch「留空即不變更」一致）。
pub fn save(
    conn: &Connection,
    base_url: &str,
    username: &str,
    password: &str,
    remote_root: &str,
    profile: &str,
) -> Result<WebdavConfig, String> {
    let (url, user, root, prof) = super::validate_config(base_url, username, remote_root, profile)?;
    let set = |k: &str, v: &str| crate::db::set_setting(conn, k, v).map_err(|e| e.to_string());
    set("cloud_base_url", &url)?;
    set("cloud_username", &user)?;
    set("cloud_remote_root", &root)?;
    set("cloud_profile", &prof)?;
    if !password.is_empty() {
        set("cloud_password", password)?;
    }
    Ok(load(conn))
}

/// 記錄上一次同步的結果（時間與本地狀態，供自動同步判斷）。
pub fn record_sync(conn: &Connection, ok: bool, message: &str) -> Result<(), String> {
    let now = crate::fsutil::now_ms();
    let set = |k: &str, v: &str| crate::db::set_setting(conn, k, v).map_err(|e| e.to_string());
    set("cloud_last_sync_ms", &now.to_string())?;
    set("cloud_last_result", message)?;
    set("cloud_last_ok", if ok { "1" } else { "0" })?;
    Ok(())
}

/// 記錄「上次上傳時的本地狀態」（自動同步用）。
pub fn record_uploaded_state(conn: &Connection, mtime_ms: i64, bytes: u64) -> Result<(), String> {
    let set = |k: &str, v: &str| crate::db::set_setting(conn, k, v).map_err(|e| e.to_string());
    set("cloud_last_mtime", &mtime_ms.to_string())?;
    set("cloud_last_bytes", &bytes.to_string())
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CloudState {
    pub provider: String,
    pub config: WebdavConfig,
    pub has_password: bool,
    pub auto_sync: bool,
    pub last_sync_ms: i64,
    pub last_result: String,
    pub last_ok: bool,
    /// 本地資料庫（mtime, 大小）—— 用來判斷「自上次上傳後有沒有變」
    pub local_mtime_ms: i64,
    pub local_bytes: u64,
    /// 自上次上傳後本地有變動 → 自動同步會上傳
    pub pending_changes: bool,
}

pub fn state(conn: &Connection, db_path: &std::path::Path) -> CloudState {
    let cfg = load(conn);
    let (mtime, bytes) = local_stat(db_path);
    let last_mtime = crate::db::get_setting(conn, "cloud_last_mtime")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let last_bytes = crate::db::get_setting(conn, "cloud_last_bytes")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    CloudState {
        provider: crate::db::get_setting(conn, "cloud_provider")
            .unwrap_or_else(|| "webdav".to_string()),
        has_password: !cfg.password.is_empty(),
        auto_sync: crate::db::get_setting(conn, "cloud_auto_sync").as_deref() == Some("1"),
        last_sync_ms: crate::db::get_setting(conn, "cloud_last_sync_ms")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        last_result: crate::db::get_setting(conn, "cloud_last_result").unwrap_or_default(),
        last_ok: crate::db::get_setting(conn, "cloud_last_ok").as_deref() == Some("1"),
        local_mtime_ms: mtime,
        local_bytes: bytes,
        pending_changes: should_auto_sync(mtime, bytes, last_mtime, last_bytes),
        config: cfg.masked(),
    }
}

/// 本地資料庫的（mtime 毫秒, 大小）。
pub fn local_stat(path: &std::path::Path) -> (i64, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (mtime, meta.len())
}
