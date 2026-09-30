//! M2：上游渠道（供應商）CRUD，對應 `providers` 表。
//!
//! 說明：`api_key` 當前為明文存儲（與 cc-switch 一致），DPAPI 級加密列入 M3 加固項。

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::fsutil::now_ms;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: i64,
    pub name: String,
    pub app_type: String,
    pub api_format: String,
    pub base_url: String,
    pub api_key: String,
    /// 上游鑑權方式：bearer（默認）| goog-key（Google x-goog-api-key）。
    pub auth_scheme: String,
    pub models: Vec<String>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub app_type: String,
    pub api_format: String,
    pub base_url: String,
    pub api_key: String,
    pub auth_scheme: String,
    pub models: Vec<String>,
    pub priority: i64,
    pub enabled: bool,
}

fn norm_scheme(s: &str) -> String {
    match s.trim() {
        "goog-key" | "anthropic" => s.trim().to_string(),
        _ => "bearer".to_string(),
    }
}

fn row_to_provider(row: &Row) -> rusqlite::Result<Provider> {
    let models_json: String = row.get(6)?;
    let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
    Ok(Provider {
        id: row.get(0)?,
        name: row.get(1)?,
        app_type: row.get(2)?,
        api_format: row.get(3)?,
        base_url: row.get(4)?,
        api_key: row.get(5)?,
        auth_scheme: row.get(9)?,
        models,
        priority: row.get(7)?,
        enabled: row.get::<_, i64>(8)? != 0,
    })
}

const SELECT: &str = "SELECT id, name, app_type, api_format, base_url, api_key, models_json, priority, enabled, auth_scheme FROM providers ORDER BY priority ASC, id ASC";

fn list_providers(conn: &Connection) -> Result<Vec<Provider>, String> {
    let mut stmt = conn.prepare(SELECT).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_provider)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn providers_list(app: AppHandle) -> Result<Vec<Provider>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        list_providers(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn insert_provider(
    conn: &Connection,
    input: &ProviderInput,
    now: i64,
) -> Result<i64, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("渠道名稱不能為空".to_string());
    }
    if input.base_url.trim().is_empty() {
        return Err("base_url 不能為空".to_string());
    }
    // 新增一律追加到末尾（priority = max+1，空表起步 0）：
    // 前端新建表單的 priority 恆為 0，若直接落庫會在重排後跳到最前。
    let next_priority: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(priority), -1) + 1 FROM providers",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            name,
            input.app_type,
            input.api_format,
            input.base_url.trim(),
            input.api_key,
            norm_scheme(&input.auth_scheme),
            serde_json::to_string(&input.models).unwrap_or_else(|_| "[]".to_string()),
            next_priority,
            if input.enabled { 1 } else { 0 },
            now,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn provider_add(db: State<DbState>, input: ProviderInput) -> Result<Provider, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let id = insert_provider(&conn, &input, now_ms())?;
    list_providers(&conn).map(|mut all| {
        all.retain(|p| p.id == id);
        all.into_iter().next().expect("just inserted")
    })
}

#[tauri::command]
pub fn provider_update(
    db: State<DbState>,
    id: i64,
    input: ProviderInput,
) -> Result<Provider, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("渠道名稱不能為空".to_string());
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let n = conn
        .execute(
            "UPDATE providers SET name=?1, app_type=?2, api_format=?3, base_url=?4, api_key=?5, auth_scheme=?6,
             models_json=?7, priority=?8, enabled=?9, updated_at=?10 WHERE id=?11",
            params![
                name,
                input.app_type,
                input.api_format,
                input.base_url.trim(),
                input.api_key,
                norm_scheme(&input.auth_scheme),
                serde_json::to_string(&input.models).unwrap_or_else(|_| "[]".to_string()),
                input.priority,
                if input.enabled { 1 } else { 0 },
                now_ms(),
                id,
            ],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("渠道不存在：{id}"));
    }
    list_providers(&conn).map(|mut all| {
        all.retain(|p| p.id == id);
        all.into_iter().next().expect("just updated")
    })
}

#[tauri::command]
pub fn provider_delete(app: AppHandle, db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let n = conn
        .execute("DELETE FROM providers WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("渠道不存在：{id}"));
    }
    // 級聯清理該來源的使用模型映射（無來源即不可用，否則淪為孤兒髒數據）。
    // 本地 Key 故意保留：請求會走明確 500「綁定的上游渠道不存在」，比靜默失效好查。
    conn.execute("DELETE FROM provider_models WHERE provider_id=?1", [id])
        .map_err(|e| e.to_string())?;
    // Phase 1：一併清掉該渠道學到的「上游拒收欄位」記憶，避免孤兒列。
    // （providers.id 是 AUTOINCREMENT，id 不會被重用，故不會誤傷新渠道；
    //   清掉純粹是不留死資料。）
    conn.execute("DELETE FROM provider_stripped_fields WHERE provider_id=?1", [id])
        .map_err(|e| e.to_string())?;
    // 同樣清掉學到的「上游協議」記憶（哪個模型在哪個端點上架）。
    conn.execute("DELETE FROM provider_model_protocol WHERE provider_id=?1", [id])
        .map_err(|e| e.to_string())?;
    drop(conn);
    // 順手清孤兒目錄（config 若仍指向它，Codex 回退內聯 models；下次接管即再生）。
    if let Ok(data) = app.path().app_data_dir() {
        let _ = std::fs::remove_file(data.join("catalogs").join(format!("codex-{id}.json")));
    }
    Ok(())
}

/// 按傳入順序重排全部來源（拖拽落點序列 → priority = 0..n-1）。
/// 校驗：ids 必須是庫內全部 id 的一個排列（無重複、無缺漏、無未知 id），
/// 否則拒寫——半截重排會產生斷層，令後續新增位置不可預期。
fn reorder_providers(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("重排序列不能為空".to_string());
    }
    let unique: std::collections::HashSet<i64> = ids.iter().copied().collect();
    if unique.len() != ids.len() {
        return Err("重排序列含重複 id".to_string());
    }
    let mut stmt = conn
        .prepare("SELECT id FROM providers")
        .map_err(|e| e.to_string())?;
    let db_ids: std::collections::HashSet<i64> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    if db_ids.len() != unique.len() || !db_ids.is_superset(&unique) {
        return Err("重排序列與當前來源列表不一致（可能已被增刪），請刷新後重試".to_string());
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (idx, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE providers SET priority=?1, updated_at=?2 WHERE id=?3",
            params![idx as i64, now_ms(), id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn provider_reorder(app: AppHandle, ids: Vec<i64>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        reorder_providers(&conn, &ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_input() -> ProviderInput {
        ProviderInput {
            name: "e2e-new".into(),
            app_type: "universal".into(),
            api_format: "openai-chat".into(),
            base_url: "https://example.com/v1".into(),
            api_key: "sk-test".into(),
            auth_scheme: "bearer".into(),
            models: vec![],
            priority: 0,
            enabled: true,
        }
    }

    /// 空來源表（open_and_ensure 會種入 seed 渠道，重排/追加斷言需從空表起步）。
    fn fresh_empty_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        conn.execute("DELETE FROM provider_models", []).unwrap();
        conn.execute("DELETE FROM providers", []).unwrap();
        (dir, conn)
    }

    #[test]
    fn insert_provider_column_count() {
        let dir = tempfile::tempdir().unwrap();
        let conn =
            crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let id = insert_provider(&conn, &test_input(), 1234567890).unwrap();
        let (name, key, created, updated, enabled): (
            String,
            String,
            i64,
            i64,
            i64,
        ) = conn
            .query_row(
                "SELECT name, api_key, created_at, updated_at, enabled FROM providers WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(name, "e2e-new");
        assert_eq!(key, "sk-test");
        assert_eq!((created, updated, enabled), (1234567890, 1234567890, 1));
    }

    #[test]
    fn insert_appends_priority() {
        // 新增一律追加到末尾：即使表單傳 priority=0，也不得插到重排後的最前。
        let (_dir, conn) = fresh_empty_db();
        let a = insert_provider(&conn, &test_input(), 1).unwrap();
        let b = insert_provider(&conn, &test_input(), 2).unwrap();
        let pr = |id: i64| -> i64 {
            conn.query_row("SELECT priority FROM providers WHERE id=?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!((pr(a), pr(b)), (0, 1));
        // 人為重排為 5、3 後再追加，仍跟在 max 之後
        conn.execute("UPDATE providers SET priority=5 WHERE id=?1", [a]).unwrap();
        conn.execute("UPDATE providers SET priority=3 WHERE id=?1", [b]).unwrap();
        let c = insert_provider(&conn, &test_input(), 3).unwrap();
        assert_eq!(pr(c), 6);
    }

    #[test]
    fn reorder_applies_full_permutation() {
        let (_dir, conn) = fresh_empty_db();
        let a = insert_provider(&conn, &test_input(), 1).unwrap();
        let b = insert_provider(&conn, &test_input(), 2).unwrap();
        let c = insert_provider(&conn, &test_input(), 3).unwrap();
        reorder_providers(&conn, &[c, a, b]).unwrap();
        let order: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM providers ORDER BY priority ASC, id ASC")
                .unwrap();
            let v: Vec<i64> = stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            v
        };
        assert_eq!(order, vec![c, a, b]);
        let priorities: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT priority FROM providers ORDER BY priority ASC")
                .unwrap();
            stmt
                .query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(priorities, vec![0, 1, 2]);
    }

    #[test]
    fn reorder_rejects_partial_and_unknown_ids() {
        let (_dir, conn) = fresh_empty_db();
        let a = insert_provider(&conn, &test_input(), 1).unwrap();
        let b = insert_provider(&conn, &test_input(), 2).unwrap();
        // 缺漏（半截重排）
        assert!(reorder_providers(&conn, &[a]).is_err());
        // 未知 id
        assert!(reorder_providers(&conn, &[a, 999, b]).is_err());
        // 重複 id
        assert!(reorder_providers(&conn, &[a, a, b]).is_err());
        // 空列
        assert!(reorder_providers(&conn, &[]).is_err());
        // 拒寫後順序不變（a,b 仍 0,1）
        let p = |id: i64| -> i64 {
            conn.query_row("SELECT priority FROM providers WHERE id=?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!((p(a), p(b)), (0, 1));
    }
}
