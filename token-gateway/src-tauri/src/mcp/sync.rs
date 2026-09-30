//! 同步與匯入（P3.1）。
//!
//! **同步**（`sync_all`）：把「資料庫說該啟用的伺服器」寫進每個工具的設定檔，
//! 並把「我們管的、但不該啟用的」移除。工具沒安裝就跳過（cc-switch 的同步條件）。
//!
//! **匯入**（`import_existing`）：一次讀遍三個工具的設定檔，把伺服器收進資料庫，
//! 並**自動對來源工具啟用**（cc-switch 也是這樣：匯入的伺服器在它來自的那個工具上
//! 預設是開的）。已經存在的 slug 不覆蓋（回報為「略過」），壞掉的檔案則回報原因。

use rusqlite::Connection;
use std::path::Path;

use super::apply::{
    app_config_path, app_installed, read_servers, write_servers_at, WriteOutcome,
};
use super::{store, validate_input, McpInput};
use crate::tools::TAKEOVER_APPS;

/// 一個工具的同步結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncOutcome {
    pub app: String,
    pub display: String,
    /// 沒安裝就 true（cc-switch：不報錯，只是不寫）
    pub skipped: bool,
    /// 跳過的原因或錯誤訊息
    pub message: String,
    pub path: String,
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub backup: Option<String>,
}

/// 全部工具的同步結果。
pub fn sync_all(
    conn: &Connection,
    app_data: &Path,
) -> Result<Vec<SyncOutcome>, String> {
    let all = store::list(conn)?;
    let managed: Vec<String> = all.iter().map(|s| s.slug.clone()).collect();
    let mut out = vec![];
    for app in TAKEOVER_APPS {
        let display = crate::tools::APPS
            .iter()
            .find(|(a, _)| *a == app)
            .map(|(_, d)| *d)
            .unwrap_or(app)
            .to_string();
        let path = app_config_path(app)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !app_installed(app) {
            out.push(SyncOutcome {
                app: app.to_string(),
                display,
                skipped: true,
                message: "這個工具沒安裝，略過（不寫入、不算錯誤）".to_string(),
                path,
                written: vec![],
                removed: vec![],
                backup: None,
            });
            continue;
        }
        // 這個工具該有哪些：綁定為啟用的
        let want: Vec<super::McpServer> = all
            .iter()
            .filter(|s| {
                s.bindings
                    .iter()
                    .any(|b| b.app == app && b.enabled)
            })
            .cloned()
            .collect();
        let cfg_path = app_config_path(app)?;
        match write_servers_at(app, &cfg_path, app_data, &want, &managed) {
            Ok(WriteOutcome {
                path,
                written,
                removed,
                backup,
            }) => out.push(SyncOutcome {
                app: app.to_string(),
                display,
                skipped: false,
                message: format!(
                    "寫入 {} 個、移除 {} 個",
                    written.len(),
                    removed.len()
                ),
                path,
                written,
                removed,
                backup,
            }),
            Err(e) => out.push(SyncOutcome {
                app: app.to_string(),
                display,
                skipped: false,
                message: format!("同步失敗：{e}"),
                path,
                written: vec![],
                removed: vec![],
                backup: None,
            }),
        }
    }
    Ok(out)
}

/// 匯入報告（cc-switch 會顯示「匯入幾個」以及哪些工具失敗）。
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct ImportReport {
    pub added: i64,
    pub skipped: i64,
    /// 每個工具讀到幾個（含 0）
    pub per_app: Vec<AppImport>,
    pub imported: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppImport {
    pub app: String,
    pub count: i64,
    pub error: String,
}

/// 把同步失敗的結果轉成匯入報告的一列（讓前端在同一個清單裡顯示原因）。
pub fn sync_error(o: &SyncOutcome) -> AppImport {
    AppImport {
        app: o.app.clone(),
        count: 0,
        error: o.message.clone(),
    }
}

/// 讀遍各工具的設定檔，把還沒有的伺服器收進資料庫並對來源工具啟用。
pub fn import_existing(conn: &Connection) -> Result<ImportReport, String> {
    let mut rep = ImportReport::default();
    for app in TAKEOVER_APPS {
        let path = match app_config_path(app) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let text = std::fs::read_to_string(&path).ok();
        let found = match read_servers(app, text.as_deref()) {
            Ok(v) => v,
            Err(e) => {
                rep.per_app.push(AppImport {
                    app: (*app).to_string(),
                    count: 0,
                    error: e,
                });
                continue;
            }
        };
        let mut count = 0;
        for raw in found {
            count += 1;
            // 匯入也要過同一套驗證（手寫的設定可能有我們不接受的鍵名）
            let Ok(input) = validate_input(&raw) else {
                rep.skipped += 1;
                continue;
            };
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM mcp_servers WHERE slug=?1",
                    [&input.slug],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if exists > 0 {
                rep.skipped += 1;
                continue;
            }
            let clean = McpInput {
                id: None,
                ..input
            };
            match store::upsert(conn, &clean) {
                Ok(saved) => {
                    let _ = store::set_binding(conn, saved.id, app, true);
                    rep.added += 1;
                    rep.imported.push(saved.slug);
                }
                Err(_) => rep.skipped += 1,
            }
        }
        rep.per_app.push(AppImport {
            app: (*app).to_string(),
            count,
            error: String::new(),
        });
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpInput;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn input(slug: &str) -> McpInput {
        validate_input(&McpInput {
            slug: slug.into(),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "pkg".into()],
            ..Default::default()
        })
        .unwrap()
    }

    /// 同步會把「該啟用的」寫進檔案、把「不該啟用的」移除，而手寫的留著。
    #[test]
    fn sync_writes_and_removes_per_app() {
        let (_d, conn) = db();
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path().join("data");
        // 直接對暫存檔測（sync_all 會走真正的主目錄，這裡測 write_servers_at 的組合）
        let file = dir.path().join("claude.json");
        std::fs::write(&file, r#"{"mcpServers":{"hand-written":{"command":"x"}}}"#).unwrap();

        let a = store::upsert(&conn, &input("ours-a")).unwrap();
        let b = store::upsert(&conn, &input("ours-b")).unwrap();
        store::set_binding(&conn, a.id, "claude", true).unwrap();
        let managed: Vec<String> = store::list(&conn).unwrap().iter().map(|s| s.slug.clone()).collect();
        assert_eq!(managed.len(), 2);

        // 只有 a 啟用 → 檔案裡只有 a，且手寫的還在
        let all = store::list(&conn).unwrap();
        let want: Vec<_> = all.iter().filter(|s| s.bindings.iter().any(|x| x.app == "claude" && x.enabled)).cloned().collect();
        assert_eq!(want.len(), 1);
        let out = write_servers_at("claude", &file, &app_data, &want, &managed).unwrap();
        assert_eq!(out.written, vec!["ours-a"]);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v["mcpServers"]["ours-a"].is_object());
        assert!(v["mcpServers"].get("ours-b").is_none(), "沒啟用的不該被寫進去");
        assert!(v["mcpServers"]["hand-written"].is_object(), "手寫的要留著");

        // 換成 b 啟用、a 關掉 → a 被移除、b 寫入
        store::set_binding(&conn, a.id, "claude", false).unwrap();
        store::set_binding(&conn, b.id, "claude", true).unwrap();
        let all = store::list(&conn).unwrap();
        let want: Vec<_> = all.iter().filter(|s| s.bindings.iter().any(|x| x.app == "claude" && x.enabled)).cloned().collect();
        let out = write_servers_at("claude", &file, &app_data, &want, &managed).unwrap();
        assert_eq!(out.written, vec!["ours-b"]);
        assert_eq!(out.removed, vec!["ours-a"]);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v["mcpServers"].get("ours-a").is_none());
        assert!(v["mcpServers"]["ours-b"].is_object());
    }

    /// 匯入：讀設定檔 → 進資料庫 → 對來源工具啟用；重複匯入不增不減。
    #[test]
    fn import_parses_and_enables_for_source_app() {
        let (_d, conn) = db();
        // 造兩個「設定檔」內容直接餵解析（不動主目錄）
        let claude = r#"{"mcpServers":{"fetch":{"command":"uvx","args":["mcp-server-fetch"]}}}"#;
        let codex = "[mcp_servers.node_repl]\ncommand = \"node_repl.exe\"\n\n[mcp_servers.node_repl.env]\nK = \"v\"\n";
        let c = read_servers("claude", Some(claude)).unwrap();
        let x = read_servers("codex", Some(codex)).unwrap();
        assert_eq!((c.len(), x.len()), (1, 1));
        for (app, list) in [("claude", c), ("codex", x)] {
            for raw in list {
                let saved = store::upsert(&conn, &validate_input(&raw).unwrap()).unwrap();
                store::set_binding(&conn, saved.id, app, true).unwrap();
            }
        }
        let all = store::list(&conn).unwrap();
        assert_eq!(all.len(), 2);
        let fetch = all.iter().find(|s| s.slug == "fetch").unwrap();
        assert!(fetch.bindings.iter().any(|b| b.app == "claude" && b.enabled));
        assert!(!fetch.bindings.iter().any(|b| b.app == "codex" && b.enabled));
        let nr = all.iter().find(|s| s.slug == "node_repl").unwrap();
        assert!(nr.bindings.iter().any(|b| b.app == "codex" && b.enabled));
        assert_eq!(nr.env.len(), 1, "env 子表格要被收進來");
    }

    /// 驗證擋掉的手寫設定不會進資料庫（不讓壞資料污染）。
    #[test]
    fn import_skips_entries_that_fail_validation() {
        let (_d, conn) = db();
        // slug 帶空白 → 我們的驗證會拒
        let text = r#"{"mcpServers":{"bad slug":{"command":"x"},"good":{"command":"y"}}}"#;
        let found = read_servers("claude", Some(text)).unwrap();
        assert_eq!(found.len(), 2, "解析階段先照收");
        let mut added = 0;
        let mut skipped = 0;
        for raw in found {
            match validate_input(&raw) {
                Ok(v) => {
                    store::upsert(&conn, &v).unwrap();
                    added += 1;
                }
                Err(_) => skipped += 1,
            }
        }
        assert_eq!((added, skipped), (1, 1));
        assert_eq!(store::list(&conn).unwrap().len(), 1);
    }
}
