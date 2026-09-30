//! MCP 伺服器與綁定的資料庫存取。

use rusqlite::{params, Connection};

use super::{Binding, HeaderPair, McpInput, McpServer};
use crate::fsutil::now_ms;
use crate::tools::TAKEOVER_APPS;

fn row_to_server(r: &rusqlite::Row) -> rusqlite::Result<McpServer> {
    let dec = |s: String| -> Vec<String> { serde_json::from_str(&s).unwrap_or_default() };
    let dec_pairs =
        |s: String| -> Vec<HeaderPair> { serde_json::from_str(&s).unwrap_or_default() };
    Ok(McpServer {
        id: r.get(0)?,
        slug: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        transport: r.get(4)?,
        command: r.get(5)?,
        args: dec(r.get(6)?),
        url: r.get(7)?,
        headers: dec_pairs(r.get(8)?),
        env: dec_pairs(r.get(9)?),
        bindings: Vec::new(),
    })
}

const SELECT: &str = "SELECT id, slug, name, description, transport, command, args_json,
                             url, headers_json, env_json
                      FROM mcp_servers";

/// 全部伺服器（含逐工具綁定狀態），依新增順序。
pub fn list(conn: &Connection) -> Result<Vec<McpServer>, String> {
    let mut stmt = conn
        .prepare(&format!("{SELECT} ORDER BY id ASC"))
        .map_err(|e| e.to_string())?;
    let mut rows: Vec<McpServer> = stmt
        .query_map([], row_to_server)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    let map = binding_map(conn)?;
    for s in &mut rows {
        s.bindings = TAKEOVER_APPS
            .iter()
            .map(|app| Binding {
                app: (*app).to_string(),
                enabled: map
                    .iter()
                    .any(|(sid, a, on)| *sid == s.id && a == app && *on),
            })
            .collect();
    }
    Ok(rows)
}

/// (server_id, app, enabled) 的清單。
pub(crate) fn binding_map(conn: &Connection) -> Result<Vec<(i64, String, bool)>, String> {
    let mut stmt = conn
        .prepare("SELECT server_id, app, enabled FROM mcp_bindings")
        .map_err(|e| e.to_string())?;
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? != 0,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> Result<McpServer, String> {
    let mut s = conn
        .query_row(&format!("{SELECT} WHERE id=?1"), [id], row_to_server)
        .map_err(|e| format!("伺服器不存在（{id}）：{e}"))?;
    let map = binding_map(conn)?;
    s.bindings = TAKEOVER_APPS
        .iter()
        .map(|app| Binding {
            app: (*app).to_string(),
            enabled: map
                .iter()
                .any(|(sid, a, on)| *sid == s.id && a == app && *on),
        })
        .collect();
    Ok(s)
}

/// 新增或更新（以 id 判斷；slug 唯一，撞名會給清楚的錯誤）。
pub fn upsert(conn: &Connection, input: &McpInput) -> Result<McpServer, String> {
    let now = now_ms();
    let args = serde_json::to_string(&input.args).unwrap_or_else(|_| "[]".into());
    let headers = serde_json::to_string(&input.headers).unwrap_or_else(|_| "[]".into());
    let env = serde_json::to_string(&input.env).unwrap_or_else(|_| "[]".into());
    let id = match input.id {
        Some(id) => {
            let n = conn
                .execute(
                    "UPDATE mcp_servers SET slug=?1, name=?2, description=?3, transport=?4,
                     command=?5, args_json=?6, url=?7, headers_json=?8, env_json=?9, updated_at=?10
                     WHERE id=?11",
                    params![
                        input.slug, input.name, input.description, input.transport,
                        input.command, args, input.url, headers, env, now, id
                    ],
                )
                .map_err(|e| {
                    if e.to_string().contains("UNIQUE") {
                        format!("Server ID「{}」已經被另一個伺服器用了", input.slug)
                    } else {
                        e.to_string()
                    }
                })?;
            if n == 0 {
                return Err(format!("伺服器不存在（{id}）"));
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO mcp_servers (slug, name, description, transport, command, args_json,
                                          url, headers_json, env_json, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
                params![
                    input.slug, input.name, input.description, input.transport,
                    input.command, args, input.url, headers, env, now
                ],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    format!("Server ID「{}」已經存在", input.slug)
                } else {
                    e.to_string()
                }
            })?;
            conn.last_insert_rowid()
        }
    };
    get(conn, id)
}

/// 刪除伺服器（連同它的綁定列）。
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM mcp_bindings WHERE server_id=?1", [id])
        .map_err(|e| e.to_string())?;
    let n = conn
        .execute("DELETE FROM mcp_servers WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("伺服器不存在（{id}）"));
    }
    Ok(())
}

/// 設定某個伺服器在某個工具上的啟用狀態。
pub fn set_binding(
    conn: &Connection,
    server_id: i64,
    app: &str,
    enabled: bool,
) -> Result<(), String> {
    if !TAKEOVER_APPS.contains(&app) {
        return Err(format!(
            "不支援的工具「{app}」（MCP 只同步給 {}）",
            TAKEOVER_APPS.join("／")
        ));
    }
    conn.execute(
        "INSERT INTO mcp_bindings (server_id, app, enabled) VALUES (?1,?2,?3)
         ON CONFLICT(server_id, app) DO UPDATE SET enabled=excluded.enabled",
        params![server_id, app, if enabled { 1 } else { 0 }],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{validate_input, McpInput};

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn input(slug: &str) -> McpInput {
        validate_input(&McpInput {
            slug: slug.into(),
            name: format!("名稱 {slug}"),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn insert_list_get_update_delete() {
        let (_d, conn) = db();
        assert!(list(&conn).unwrap().is_empty());
        let a = upsert(&conn, &input("mcp-a")).unwrap();
        assert_eq!(a.slug, "mcp-a");
        assert_eq!(a.name, "名稱 mcp-a");
        assert_eq!(a.args, vec!["-y", "pkg"]);
        // 每個伺服器都帶三個工具的綁定狀態（預設全關）
        assert_eq!(a.bindings.len(), 3);
        assert!(a.bindings.iter().all(|b| !b.enabled));

        // 更新
        let mut up = input("mcp-a");
        up.id = Some(a.id);
        up.transport = "http".into();
        up.url = "https://example.com/mcp".into();
        up.command = String::new();
        let b = upsert(&conn, &up).unwrap();
        assert_eq!(b.id, a.id);
        assert_eq!(b.transport, "http");
        assert_eq!(b.url, "https://example.com/mcp");

        // 刪除
        delete(&conn, a.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert!(delete(&conn, a.id).unwrap_err().contains("不存在"));
    }

    #[test]
    fn duplicate_slug_is_rejected_with_a_clear_message() {
        let (_d, conn) = db();
        upsert(&conn, &input("same")).unwrap();
        let err = upsert(&conn, &input("same")).unwrap_err();
        assert!(err.contains("已經存在"), "{err}");
        // 改成別的 id 也不能撞（UNIQUE 是 slug）
        let mut other = input("other");
        other.id = Some(999);
        assert!(upsert(&conn, &other).is_err());
    }

    #[test]
    fn bindings_roundtrip_and_unknown_app_is_rejected() {
        let (_d, conn) = db();
        let s = upsert(&conn, &input("with-bind")).unwrap();
        set_binding(&conn, s.id, "codex", true).unwrap();
        set_binding(&conn, s.id, "claude", true).unwrap();
        set_binding(&conn, s.id, "claude", false).unwrap();
        let got = get(&conn, s.id).unwrap();
        let on = |app: &str| {
            got.bindings
                .iter()
                .find(|b| b.app == app)
                .map(|b| b.enabled)
                .unwrap_or(false)
        };
        assert!(!on("claude"), "關掉要生效");
        assert!(on("codex"));
        assert!(!on("opencode"));
        // 不可同步的工具要擋（使用者裁示不新增受管工具）
        let err = set_binding(&conn, s.id, "cursor", true).unwrap_err();
        assert!(err.contains("不支援的工具"), "{err}");
        let err = set_binding(&conn, s.id, "hermes", true).unwrap_err();
        assert!(err.contains("不支援的工具"), "{err}");
    }

    #[test]
    fn delete_also_removes_bindings() {
        let (_d, conn) = db();
        let s = upsert(&conn, &input("x")).unwrap();
        set_binding(&conn, s.id, "codex", true).unwrap();
        delete(&conn, s.id).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM mcp_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "綁定列不可以留下來當孤兒");
    }
}
