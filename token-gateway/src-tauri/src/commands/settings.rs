//! 應用程式設定命令（原 `commands.rs` 的「設定」段）。

use serde::Serialize;
use tauri::State;

use crate::db::{self, DbState};
use crate::proxy::{self, ProxyState};

// ---------------------------------------------------------------- 設定 ---

#[derive(Serialize)]
pub struct AppSettings {
    pub gateway_port: u16,
    pub auto_start_proxy: bool,
    pub accent: String,
}

const ACCENT_IDS: &[&str] = &["blue", "green", "purple", "pink"];

#[tauri::command]
pub fn settings_get(db: State<DbState>) -> Result<AppSettings, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let port = db::get_setting(&conn, "gateway_port")
        .and_then(|v| v.parse().ok())
        .unwrap_or(db::DEFAULT_GATEWAY_PORT);
    let auto = db::get_setting(&conn, "auto_start_proxy")
        .is_some_and(|v| v == "1");
    let accent = db::get_setting(&conn, "accent")
        .filter(|v| ACCENT_IDS.contains(&v.as_str()))
        .unwrap_or_else(|| "blue".to_string());
    Ok(AppSettings {
        gateway_port: port,
        auto_start_proxy: auto,
        accent,
    })
}

/// 外觀強調色持久化（白名單校驗；SQLite 提交即落盤，不受 WebView 儲存刷盤時機影響）
#[tauri::command]
pub fn settings_set_accent(db: State<DbState>, accent: String) -> Result<String, String> {
    if !ACCENT_IDS.contains(&accent.as_str()) {
        return Err(format!("未知的強調色：{accent}"));
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "accent", &accent).map_err(|e| e.to_string())?;
    Ok(accent)
}

#[tauri::command]
pub fn settings_set(
    db: State<DbState>,
    proxy: State<ProxyState>,
    gateway_port: u16,
    auto_start_proxy: bool,
) -> Result<AppSettings, String> {
    if !(1..=65535).contains(&gateway_port) {
        return Err("連接埠範圍應為 1–65535".to_string());
    }
    {
        let inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        if let Some(r) = inner.as_ref() {
            if r.port != gateway_port {
                return Err(format!(
                    "網關正在連接埠 {} 執行，請先停止再修改連接埠",
                    r.port
                ));
            }
        }
    }
    proxy::check_port(gateway_port)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "gateway_port", &gateway_port.to_string())
        .map_err(|e| e.to_string())?;
    db::set_setting(
        &conn,
        "auto_start_proxy",
        if auto_start_proxy { "1" } else { "0" },
    )
    .map_err(|e| e.to_string())?;
    drop(conn);
    settings_get(db)
}
