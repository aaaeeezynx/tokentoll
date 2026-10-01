//! `cloudsync` 的測試（純函式與設定層；HTTP 的實機驗證在 §0.9.31 用本機 WebDAV 伺服器做）。

use super::*;

fn db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("app.db")).unwrap();
    (dir, conn)
}

#[test]
fn config_validation_normalizes_and_rejects() {
    let (url, user, root, prof) = validate_config(
        " https://dav.example.com/remote.php/dav/files/me/ ",
        " me ",
        "",
        "",
    )
    .unwrap();
    assert_eq!(url, "https://dav.example.com/remote.php/dav/files/me");
    assert_eq!(user, "me");
    assert_eq!(root, DEFAULT_ROOT, "空值用預設（與 cc-switch 相同）");
    assert_eq!(prof, DEFAULT_PROFILE);

    // 網址一定要 http(s)
    assert!(validate_config("ftp://x", "u", "", "").unwrap_err().contains("http(s)"));
    // 帳號必填
    assert!(validate_config("https://x", "  ", "", "").unwrap_err().contains("帳號"));
    // 路徑不可以跳脫
    assert!(validate_config("https://x", "u", "../etc", "").unwrap_err().contains("不合法"));
    assert!(validate_config("https://x", "u", "a/../b", "").unwrap_err().contains("不合法"));
    // 多層路徑可以
    let (_, _, r2, p2) = validate_config("https://x", "u", "sync/backups", "device-a").unwrap();
    assert_eq!((r2.as_str(), p2.as_str()), ("sync/backups", "device-a"));
}

#[test]
fn dir_url_is_joined_cleanly() {
    assert_eq!(
        dir_url("https://dav.example.com/", "/token-gateway-sync/", "/default/"),
        "https://dav.example.com/token-gateway-sync/default"
    );
}

#[test]
fn auto_sync_triggers_only_on_change() {
    // 從沒上傳過 → 值得同步
    assert!(should_auto_sync(1000, 2048, 0, 0));
    // 一模一樣 → 不同步
    assert!(!should_auto_sync(1000, 2048, 1000, 2048));
    // 時間變了 → 同步
    assert!(should_auto_sync(2000, 2048, 1000, 2048));
    // 大小變了 → 同步
    assert!(should_auto_sync(1000, 4096, 1000, 2048));
    // 讀不到本地檔（mtime 0）→ 不同步
    assert!(!should_auto_sync(0, 0, 1000, 2048));
}

#[test]
fn password_is_masked() {
    assert_eq!(mask_password(""), "（未設定）");
    assert_eq!(mask_password("abcd"), "••••");
    assert_eq!(mask_password(&"x".repeat(40)).chars().count(), 12, "最多 12 個點");
}

#[test]
fn config_roundtrip_and_blank_password_keeps_old() {
    let (_d, conn) = db();
    let cfg = store::save(
        &conn,
        "https://dav.example.com",
        "me",
        "app-password",
        "",
        "",
    )
    .unwrap();
    assert_eq!(cfg.password, "app-password");
    assert!(cfg.is_configured());
    // 密碼遮罩版不含明文
    let masked = cfg.masked();
    assert!(!masked.password.contains("app-password"));
    assert!(masked.password.contains('•'));

    // 留空＝不變更（cc-switch 的行為）
    let cfg2 = store::save(&conn, "https://dav2.example.com", "me2", "", "", "").unwrap();
    assert_eq!(cfg2.password, "app-password", "留空不覆蓋舊密碼");
    assert_eq!(cfg2.base_url, "https://dav2.example.com");
}

#[test]
fn state_reports_pending_changes_and_last_result() {
    let (dir, conn) = db();
    let db_path = dir.path().join("app.db");
    let st = store::state(&conn, &db_path);
    assert!(!st.auto_sync, "預設不自動同步");
    assert_eq!(st.provider, "webdav");
    assert!(!st.has_password);
    assert_eq!(st.last_sync_ms, 0);
    assert!(st.pending_changes, "從沒同步過 → 有待同步變更");

    store::save(&conn, "https://dav.example.com", "me", "pw", "", "").unwrap();
    store::record_uploaded_state(&conn, st.local_mtime_ms, st.local_bytes).unwrap();
    let st2 = store::state(&conn, &db_path);
    assert!(st2.has_password);
    assert!(!st2.pending_changes, "剛上傳過 → 沒有待同步變更");

    store::record_sync(&conn, true, "已上傳").unwrap();
    let st3 = store::state(&conn, &db_path);
    assert!(st3.last_ok);
    assert_eq!(st3.last_result, "已上傳");
    assert!(st3.last_sync_ms > 0);
}
