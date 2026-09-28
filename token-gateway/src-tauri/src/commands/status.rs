//! 狀態查詢與網關啟停命令（原 `commands.rs` 的「狀態」段）。

use serde::Serialize;
use tauri::State;

use crate::db::{DbState, SCHEMA_VERSION};
use crate::proxy::{self, ProxyState};

#[derive(Serialize)]
pub struct DbStatus {
    pub path: String,
    pub schema_version: i32,
    pub provider_count: i64,
    pub key_count: i64,
    pub log_count: i64,
}

#[tauri::command]
pub fn db_status(db: State<DbState>) -> Result<DbStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    Ok(DbStatus {
        path: db.path.to_string_lossy().to_string(),
        schema_version: SCHEMA_VERSION,
        provider_count: count("SELECT COUNT(*) FROM providers"),
        key_count: count("SELECT COUNT(*) FROM local_keys"),
        log_count: count("SELECT COUNT(*) FROM request_logs"),
    })
}

#[derive(Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub listen: String,
    pub port: u16,
    pub started_at: Option<i64>,
}

/// 真實狀態：讀 ProxyState（M1 存根已替換）。
#[tauri::command]
pub fn proxy_status(proxy: State<ProxyState>) -> ProxyStatus {
    let inner = proxy.inner.lock().unwrap_or_else(|e| e.into_inner());
    match inner.as_ref() {
        Some(r) => ProxyStatus {
            running: true,
            listen: "127.0.0.1".to_string(),
            port: r.port,
            started_at: Some(r.started_at),
        },
        None => ProxyStatus {
            running: false,
            listen: "127.0.0.1".to_string(),
            port: 0,
            started_at: None,
        },
    }
}

/// C 方案：僅檢測連接埠是否可用，不啟動。
#[tauri::command]
pub fn proxy_check_port(port: u16) -> Result<(), String> {
    proxy::check_port(port)
}

#[tauri::command]
pub async fn proxy_start(
    db: State<'_, DbState>,
    proxy: State<'_, ProxyState>,
    port: u16,
) -> Result<ProxyStatus, String> {
    if !(1..=65535).contains(&port) {
        return Err("連接埠範圍應為 1–65535".to_string());
    }
    {
        let inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        if let Some(r) = inner.as_ref() {
            if r.port == port {
                return Ok(ProxyStatus {
                    running: true,
                    listen: "127.0.0.1".to_string(),
                    port,
                    started_at: Some(r.started_at),
                });
            }
            return Err(format!(
                "網關已在連接埠 {} 執行，請先停止再切換連接埠",
                r.port
            ));
        }
    }
    proxy::check_port(port)?;
    let db_path = db.path.clone();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                format!("連接埠 {port} 已被佔用（本機可能有 cc-switch 或其他代理在執行）")
            } else {
                format!("連接埠 {port} 綁定失敗：{e}")
            }
        })?;
    let handle = tokio::spawn(async move {
        if let Err(e) = proxy::serve(db_path, listener).await {
            eprintln!("gateway error: {e}");
        }
    });
    {
        let mut inner = proxy.inner.lock().map_err(|e| e.to_string())?;
        *inner = Some(proxy::RunningProxy {
            port,
            started_at: crate::fsutil::now_ms(),
            handle,
        });
    }
    Ok(ProxyStatus {
        running: true,
        listen: "127.0.0.1".to_string(),
        port,
        started_at: None,
    })
}

#[tauri::command]
pub fn proxy_stop(proxy: State<ProxyState>) -> Result<(), String> {
    let mut inner = proxy.inner.lock().map_err(|e| e.to_string())?;
    if let Some(r) = inner.take() {
        r.handle.abort();
    }
    Ok(())
}
