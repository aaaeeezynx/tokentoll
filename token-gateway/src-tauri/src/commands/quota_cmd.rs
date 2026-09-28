//! 訂閱額度查詢命令（原 `commands.rs` 的「訂閱額度」段）。

use tauri::{AppHandle, Manager};

use crate::db::DbState;

// ---------------------------------------------------------------- 訂閱額度 ---

/// 查詢訂閱方案額度（OpenCode Go / Claude CLI / Codex OAuth）：用量百分比 + 重置時間。
/// 只讀本地憑證與官方查詢接口，憑證不回前端；網路在 spawn_blocking，鎖只覆蓋讀取。
#[tauri::command]
pub async fn quota_query_all(app: AppHandle) -> Vec<crate::quota::SubscriptionQuota> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        let providers: Vec<(i64, String, String, String, bool)> = {
            let Ok(conn) = db.conn.lock() else {
                return vec![];
            };
            let Ok(mut stmt) = conn.prepare(
                "SELECT id, name, base_url, api_key, enabled FROM providers
                 ORDER BY priority ASC, id ASC",
            ) else {
                return vec![];
            };
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            });
            match rows {
                Ok(it) => it.filter_map(|r| r.ok()).collect(),
                Err(_) => vec![],
            }
        };
        crate::quota::query_all(&providers)
    })
    .await
    .unwrap_or_default()
}
