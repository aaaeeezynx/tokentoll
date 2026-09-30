//! 協定註冊與「已經開著」時的轉交（P4.1）。
//!
//! ## 為什麼是 HKCU
//!
//! cc-switch 由安裝程式寫 `HKEY_CLASSES_ROOT\ccswitch`（要管理員權限）。
//! 我們改成寫 **`HKCU\Software\Classes\tokengateway`** —— 同樣能讓 Windows 認得
//! `tokengateway://`，而且**不需要系統管理員**，App 自己就能在啟動時補上。
//!
//! ## 已經開著的時候
//!
//! 沒有 single-instance 外掛時，點連結會**再開一個行程**（兩個行程搶同一個埠與
//! SQLite 並不理想）。所以：新的行程不會啟動 App，而是把連結寫進
//! `<app_data>/deeplink-inbox.json` 就結束；已經在跑的那個行程有一個小執行緒
//! 每秒看一次收件匣，看到就發出 `deeplink` 事件給前端（與托盤的作法是同一套）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::SCHEME;

/// 收件匣檔名。
const INBOX: &str = "deeplink-inbox.json";

/// 是不是已經有實例在跑？
///
/// 用**閘道埠**當作身分：這個 App 一定會綁 `DEFAULT_GATEWAY_PORT`，所以「綁不上」
/// 就代表已經有一個在跑。比 PID 檔或具名 mutex 簡單，也沒有殘留檔的問題
/// （當機留下的檔案會讓人永遠開不起來）。
pub fn another_instance_running() -> bool {
    std::net::TcpListener::bind(("127.0.0.1", crate::db::DEFAULT_GATEWAY_PORT)).is_err()
}

/// 每秒看一次收件匣，有新連結就發 `deeplink` 事件給前端。
///
/// 與托盤的重建執行緒同一套作法（輪詢、失敗不致命）。
pub fn spawn_watcher(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{Emitter, Manager};
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1500));
        let Ok(dir) = handle.path().app_data_dir() else {
            continue;
        };
        if let Some(url) = take_inbox(&dir) {
            let _ = handle.emit("deeplink", url);
        }
    });
    Ok(())
}

/// 註冊協定（寫 HKCU；重複註冊是安全的）。
///
/// 回傳 Ok(true) 表示這次真的寫了（之前沒有或 exe 路徑變了）。
pub fn ensure_registered(exe: &Path) -> Result<bool, String> {
    let exe_s = exe.to_string_lossy().to_string();
    let current = registered_command().unwrap_or_default();
    if current.contains(&exe_s) {
        return Ok(false); // 已經指到現在這個 exe
    }
    let base = format!("HKCU\\Software\\Classes\\{SCHEME}");
    let cmds: Vec<Vec<String>> = vec![
        vec![
            "add".into(),
            base.clone(),
            "/ve".into(),
            "/d".into(),
            "URL:Token Gateway Deep Link".into(),
            "/f".into(),
        ],
        vec![
            "add".into(),
            base.clone(),
            "/v".into(),
            "URL Protocol".into(),
            "/d".into(),
            "".into(),
            "/f".into(),
        ],
        vec![
            "add".into(),
            format!("{base}\\shell\\open\\command"),
            "/ve".into(),
            "/d".into(),
            format!("\"{exe_s}\" \"%1\""),
            "/f".into(),
        ],
    ];
    for args in cmds {
        let out = std::process::Command::new("reg")
            .args(&args)
            .output()
            .map_err(|e| format!("執行 reg 失敗：{e}"))?;
        if !out.status.success() {
            return Err(format!(
                "註冊協定失敗：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    Ok(true)
}

/// 目前註冊的開啟命令（沒註冊就回 None）。
pub fn registered_command() -> Option<String> {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            &format!("HKCU\\Software\\Classes\\{SCHEME}\\shell\\open\\command"),
            "/ve",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|l| l.contains("REG_SZ"))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|s| s.trim().to_string())
}

/// 解除註冊（設定頁的「取消註冊」用；刪掉整棵機碼）。
pub fn unregister() -> Result<(), String> {
    let out = std::process::Command::new("reg")
        .args([
            "delete",
            &format!("HKCU\\Software\\Classes\\{SCHEME}"),
            "/f",
        ])
        .output()
        .map_err(|e| format!("執行 reg 失敗：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// ------------------------------------------------------------ 收件匣 ---

fn inbox_path(app_data: &Path) -> PathBuf {
    app_data.join(INBOX)
}

/// 新的行程把連結留在這裡就結束（讓已經在跑的那個去處理）。
pub fn write_inbox(app_data: &Path, url: &str) -> Result<(), String> {
    std::fs::create_dir_all(app_data).map_err(|e| e.to_string())?;
    std::fs::write(inbox_path(app_data), url).map_err(|e| format!("寫入收件匣失敗：{e}"))
}

/// 讀取並清掉收件匣（回傳待處理的連結）。
pub fn take_inbox(app_data: &Path) -> Option<String> {
    let p = inbox_path(app_data);
    let url = std::fs::read_to_string(&p).ok()?;
    let url = url.trim().to_string();
    let _ = std::fs::remove_file(&p);
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_roundtrip_and_clear() {
        let d = tempfile::tempdir().unwrap();
        assert!(take_inbox(d.path()).is_none(), "一開始是空的");
        write_inbox(d.path(), "tokengateway://v1/import?resource=skill&repo=a/b").unwrap();
        let got = take_inbox(d.path()).unwrap();
        assert!(got.contains("resource=skill"));
        assert!(take_inbox(d.path()).is_none(), "讀過就要清掉（不重複匯入）");
        // 空白內容視為沒有
        write_inbox(d.path(), "   ").unwrap();
        assert!(take_inbox(d.path()).is_none());
    }

    /// 只讀目前狀態，不動使用者機器上的登錄檔（查詢是安全的）。
    #[test]
    fn query_does_not_change_anything() {
        let before = registered_command();
        let after = registered_command();
        assert_eq!(before, after);
    }
}
