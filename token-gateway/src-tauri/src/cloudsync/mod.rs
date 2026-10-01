//! 雲端同步（P4.10，對標 cc-switch 的 Cloud Sync → WebDAV）。
//!
//! ## cc-switch 的實際行為（照它手冊 §1.5 抄）
//!
//! - 用 **WebDAV** 或 S3 在裝置之間同步設定（**同時只能開一種**）。
//! - 設定項：伺服器網址、帳號、密碼、**遠端根目錄**（預設 `cc-switch-sync`）、
//!   **設定檔名稱**（預設 `default`）、**自動同步**。
//! - 操作：**測試連線**、**上傳**（本地 → 遠端）、**下載**（遠端 → 本地，
//!   下載前顯示遠端快照資訊，且**下載前會自動備份本地資料庫**）。
//! - 上傳會覆蓋遠端、下載會覆蓋本地 —— 手冊特別提醒這件事。
//!
//! ## 我們做的
//!
//! - **只做 WebDAV**（S3 需要自簽 SigV4，且我們沒有可驗證的 S3 端點；
//!   寧缺勿假，矩陣上標明）。
//! - 遠端佈局：`<根目錄>/<設定檔>/tokens.db` ＋ `<根目錄>/<設定檔>/manifest.json`
//!   （manifest 有協定版本、schema 版本、App 版本、時間、大小 —— 下載前顯示給使用者看）。
//! - **下載前一定先做本地安全備份**（沿用 `dbbackup::create`），然後用 SQLite 的
//!   線上備份 API 把下載回來的檔案寫回**現有連線**（與還原同一條路徑）。
//! - 密碼存在 `settings`（與 cc-switch 一樣存在自己的資料庫裡），
//!   **UI 只顯示遮罩後的值**，且驗證時不會回傳明文。

pub(crate) mod store;
pub(crate) mod webdav;

pub use store::{CloudState, WebdavConfig};
pub use webdav::{apply_remote_db, fetch_remote_db, test_connection, upload, RemoteSnapshot, SyncOutcome};

/// 協定版本（我們自己的；cc-switch 是 v2）。
pub(crate) const PROTOCOL: &str = "tokengateway-sync/1";
/// 預設遠端根目錄（cc-switch 用 `cc-switch-sync`）。
pub(crate) const DEFAULT_ROOT: &str = "token-gateway-sync";
/// 預設設定檔名稱（cc-switch 用 `default`）。
pub(crate) const DEFAULT_PROFILE: &str = "default";
/// 遠端檔案名稱。
pub(crate) const DB_FILE: &str = "tokens.db";
pub(crate) const MANIFEST_FILE: &str = "manifest.json";

/// 驗證設定（同時組出正規化後的值）。
pub fn validate_config(
    base_url: &str,
    username: &str,
    remote_root: &str,
    profile: &str,
) -> Result<(String, String, String, String), String> {
    let url = base_url.trim().trim_end_matches('/').to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!("伺服器網址必須是 http(s)：{base_url}"));
    }
    let user = username.trim().to_string();
    if user.is_empty() {
        return Err("請填 WebDAV 帳號".to_string());
    }
    let clean = |s: &str, fallback: &str| -> Result<String, String> {
        let v = s.trim().trim_matches('/');
        if v.is_empty() {
            return Ok(fallback.to_string());
        }
        if v.split('/').any(|seg| seg == ".." || seg == "." || seg.is_empty()) {
            return Err(format!("路徑不合法：{s}"));
        }
        Ok(v.to_string())
    };
    Ok((
        url,
        user,
        clean(remote_root, DEFAULT_ROOT)?,
        clean(profile, DEFAULT_PROFILE)?,
    ))
}

/// 遠端資料夾的完整網址。
pub(crate) fn dir_url(base_url: &str, root: &str, profile: &str) -> String {
    format!(
        "{}/{}/{}",
        base_url.trim_end_matches('/'),
        root.trim_matches('/'),
        profile.trim_matches('/')
    )
}

/// 自動同步要不要跑：本地資料庫的（mtime, 大小）與上次上傳的不同就值得同步。
pub(crate) fn should_auto_sync(
    local_mtime_ms: i64,
    local_bytes: u64,
    last_mtime_ms: i64,
    last_bytes: u64,
) -> bool {
    local_mtime_ms > 0 && (local_mtime_ms != last_mtime_ms || local_bytes != last_bytes)
}

/// 遮罩密碼（給 UI；驗證時不回傳明文）。
pub(crate) fn mask_password(p: &str) -> String {
    let n = p.chars().count();
    if n == 0 {
        "（未設定）".to_string()
    } else {
        "•".repeat(n.min(12))
    }
}

#[cfg(test)]
mod tests;
