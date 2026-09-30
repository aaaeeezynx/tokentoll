//! `tray` 的測試：選單模型（哪些工具、哪些來源）與項目 id 的解析。

use super::*;

fn db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    conn.execute("DELETE FROM provider_models", []).unwrap();
    conn.execute("DELETE FROM providers", []).unwrap();
    (dir, conn)
}

fn add_provider(conn: &Connection, name: &str, enabled: i64) -> i64 {
    let pr: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(priority), -1) + 1 FROM providers",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                models_json, priority, enabled, created_at, updated_at)
         VALUES (?1,'universal','openai-chat','https://example.com/v1','k','bearer','[]',?2,?3,0,0)",
        rusqlite::params![name, pr, enabled],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn add_model(conn: &Connection, pid: i64, name: &str, enabled: i64) {
    conn.execute(
        "INSERT INTO provider_models (provider_id, display_name, actual_model, ord, enabled)
         VALUES (?1,?2,?2,0,?3)",
        rusqlite::params![pid, name, enabled],
    )
    .unwrap();
}

/// 選單只列**可接管**的三個工具，且每個工具都列出可用來源。
#[test]
fn menu_lists_only_takeover_tools() {
    let (_d, conn) = db();
    let a = add_provider(&conn, "oc-go", 1);
    add_model(&conn, a, "m", 1);
    let model = menu_model(&conn).unwrap();
    assert_eq!(
        model.iter().map(|t| t.app.as_str()).collect::<Vec<_>>(),
        vec!["claude", "codex", "opencode"],
        "只該有三個可接管工具（不是全部七個）"
    );
    assert_eq!(model[0].display, "Claude Code");
    for t in &model {
        assert_eq!(
            t.providers,
            vec![TrayProvider {
                id: a,
                name: "oc-go".into()
            }]
        );
    }
}

/// 列出來的來源必須是「點得動」的：啟用中、且至少有一個啟用模型。
#[test]
fn unusable_providers_are_hidden() {
    let (_d, conn) = db();
    let ok = add_provider(&conn, "ok", 1);
    add_model(&conn, ok, "m", 1);
    let disabled = add_provider(&conn, "disabled", 0);
    add_model(&conn, disabled, "m", 1);
    let no_model = add_provider(&conn, "no-model", 1);
    let all_models_off = add_provider(&conn, "models-off", 1);
    add_model(&conn, all_models_off, "m", 0);
    let _ = no_model;

    let model = menu_model(&conn).unwrap();
    let codex = model.iter().find(|t| t.app == "codex").unwrap();
    assert_eq!(
        codex.providers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["ok"],
        "停用的來源、沒有模型的來源、模型全停用的來源都不該出現"
    );
}

/// 順序＝來源清單順序（與請求路徑的候選順序一致）。
#[test]
fn providers_follow_priority_order() {
    let (_d, conn) = db();
    let a = add_provider(&conn, "first", 1);
    let b = add_provider(&conn, "second", 1);
    add_model(&conn, a, "m", 1);
    add_model(&conn, b, "m", 1);
    let model = menu_model(&conn).unwrap();
    assert_eq!(
        model[0]
            .providers
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
}

/// 事件名稱與 payload 鍵名是**跨語言的契約**（Rust emit ↔ TS listen），用測試釘住。
///
/// 為什麼值得一條測試：這兩邊沒有共同編譯器，打錯字只會讓「托盤點了沒反應」，
/// 而且不會有任何錯誤訊息。
#[test]
fn switch_event_contract_is_stable() {
    assert_eq!(EVENT_SWITCH, "tray-switch");
    let payload = serde_json::json!({"app": "codex", "provider_id": 22});
    let obj = payload.as_object().unwrap();
    let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["app", "provider_id"], "鍵名必須與 TS 的 TraySwitch 一致");
    assert_eq!(obj["app"], "codex");
    assert_eq!(obj["provider_id"], 22);
}

/// 項目 id 來回轉換；壞東西一律回 None（不能讓它變成「寫到某個工具」）。
#[test]
fn item_id_roundtrip_and_rejects_junk() {
    assert_eq!(item_id("codex", 22), "sw:codex:22");
    assert_eq!(
        parse_item_id("sw:codex:22"),
        Some(("codex".to_string(), 22))
    );
    assert_eq!(
        parse_item_id("sw:claude:1"),
        Some(("claude".to_string(), 1))
    );
    for bad in [
        "show",
        "quit",
        "",
        "sw:",
        "sw:codex",
        "sw:codex:abc",
        "sw:codex:0",
        "sw:codex:-3",
        "sw:hermes:5",   // 不可接管的工具
        "sw:cursor:5",   // 同上
        "sw:unknown:5",
        "empty:codex",
    ] {
        assert_eq!(parse_item_id(bad), None, "{bad:?} 不該被當成切換項");
    }
}
