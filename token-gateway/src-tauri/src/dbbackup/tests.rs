//! `dbbackup` 的測試。
//!
//! 最要緊的一條是 `restore_brings_the_old_data_back`：先備份、改資料、還原，
//! 然後檢查**改動真的被還原回去**（不是只有檔案存在而已）。

use super::*;

fn tmp_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("app.db")).unwrap();
    (dir, conn)
}

fn add_provider(conn: &Connection, name: &str) {
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                models_json, priority, enabled, created_at, updated_at)
         VALUES (?1, 'universal', 'openai-chat', 'https://x', 'k', 'bearer', '[]', 0, 1, 0, 0)",
        [name],
    )
    .unwrap();
}

fn count_providers(conn: &Connection, name: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM providers WHERE name=?1",
        [name],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn name_matches_the_documented_format() {
    // 名稱用**本地時間**（使用者看到的是自己的牆上時間），所以用本地時區反推
    let now = crate::fsutil::now_ms();
    let name = default_name(now);
    assert!(name.starts_with(PREFIX), "{name}");
    assert_eq!(name.len(), PREFIX.len() + 15, "YYYYMMDD_HHMMSS：{name}");
    let parsed = created_from_name(&name);
    // 同一秒內建立 → 解析回來應該幾乎相同（本地時區來回一致）
    assert!((parsed - now).abs() < 1000, "{parsed} vs {now}");
    // 解析不出來的名字回 0（不 panic）
    assert_eq!(created_from_name("random"), 0);
}

#[test]
fn name_validation_blocks_paths() {
    for ok in ["db_backup_20260315_143000", "my-backup", "a.b"] {
        assert!(validate_name(ok).is_ok(), "{ok} 應該可以");
    }
    for bad in ["", "  ", "../evil", "a/b", "a\\b", ".hidden", &"x".repeat(200)] {
        assert!(validate_name(bad).is_err(), "{bad:?} 應該被拒");
    }
}

#[test]
fn prune_keeps_the_newest() {
    let mk = |name: &str, ms: i64| BackupInfo {
        name: name.to_string(),
        created_ms: ms,
        ..Default::default()
    };
    let items = vec![
        mk("oldest", 100),
        mk("middle", 200),
        mk("newest", 300),
    ];
    assert_eq!(prune_plan(items.clone(), 2), vec!["oldest"]);
    assert_eq!(prune_plan(items.clone(), 3), Vec::<String>::new());
    assert_eq!(prune_plan(items, 1), vec!["middle", "oldest"]);
    // 保留數至少 1（不會把全部都刪掉）
    let one = vec![mk("a", 1)];
    assert!(prune_plan(one, 0).is_empty());
}

#[test]
fn due_logic_follows_the_interval() {
    let hour = 3_600_000;
    assert!(!is_due(0, 0, 999_999), "停用時永遠不備份");
    assert!(is_due(0, 24, 999_999), "從沒備份過 → 立刻做");
    assert!(!is_due(1_000_000, 6, 1_000_000 + hour), "還沒到間隔");
    assert!(is_due(1_000_000, 6, 1_000_000 + 6 * hour), "剛好到間隔");
    assert!(is_due(1_000_000, 24, 1_000_000 + 48 * hour), "超過間隔");
}

#[test]
fn create_list_rename_delete_roundtrip() {
    let (dir, conn) = tmp_db();
    let app_data = dir.path();

    let b = create(&conn, app_data, None).unwrap();
    assert!(std::path::Path::new(&b.path).is_file());
    assert!(b.bytes > 0, "備份檔不該是空的");
    let listed = list(app_data);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, b.name);

    // 自訂名稱 + 改名
    let named = create(&conn, app_data, Some("before_upgrade".to_string())).unwrap();
    assert_eq!(named.name, "before_upgrade");
    let renamed = rename(app_data, "before_upgrade", "keep_this").unwrap();
    assert_eq!(renamed.name, "keep_this");
    assert!(!list(app_data).iter().any(|b| b.name == "before_upgrade"));
    assert!(rename(app_data, "nope", "x").unwrap_err().contains("找不到"));

    delete(app_data, "keep_this").unwrap();
    assert_eq!(list(app_data).len(), 1);
    assert!(delete(app_data, "keep_this").unwrap_err().contains("找不到"));
    // 目錄裡不該留下非 .db 的東西被當成備份
    std::fs::write(backup_dir(app_data).join("notes.txt"), "x").unwrap();
    assert_eq!(list(app_data).len(), 1, "只列 .db");
}

/// **核心測試**：還原真的把資料帶回來（而且還原前會自動做安全備份）。
#[test]
fn restore_brings_the_old_data_back() {
    let (dir, mut conn) = tmp_db();
    let app_data = dir.path();

    add_provider(&conn, "keep_me");
    assert_eq!(count_providers(&conn, "keep_me"), 1);
    let snapshot = create(&conn, app_data, Some("snapshot".to_string())).unwrap();
    assert!(std::path::Path::new(&snapshot.path).is_file());

    // 備份之後才加的來源：還原後應該消失
    add_provider(&conn, "added_later");
    assert_eq!(count_providers(&conn, "added_later"), 1);

    let out = restore(&mut conn, app_data, "snapshot").unwrap();
    assert_eq!(out.restored, "snapshot");
    assert!(!out.safety_backup.is_empty(), "要有安全備份");
    assert!(out.note.contains("schema v"), "{}", out.note);

    assert_eq!(count_providers(&conn, "keep_me"), 1, "原本的資料還在");
    assert_eq!(count_providers(&conn, "added_later"), 0, "備份之後的改動被還原掉");
    // 安全備份存在，而且裡面有「added_later」
    let safety = Connection::open(
        backup_dir(app_data).join(format!("{}.db", out.safety_backup)),
    )
    .unwrap();
    assert_eq!(count_providers(&safety, "added_later"), 1, "安全備份是還原前的狀態");

    // 還原後連線仍然可用（可以繼續寫）
    add_provider(&conn, "after_restore");
    assert_eq!(count_providers(&conn, "after_restore"), 1);
    assert!(restore(&mut conn, app_data, "nope").unwrap_err().contains("找不到"));
}

#[test]
fn retention_prunes_on_create() {
    let (dir, conn) = tmp_db();
    let app_data = dir.path();
    // 保留 3 份
    crate::db::set_setting(&conn, "db_backup_keep", "3").unwrap();
    // 造 5 份（名稱各不相同，時間也遞增）
    for i in 0..5 {
        create(&conn, app_data, Some(format!("b{i}"))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let names: Vec<String> = list(app_data).into_iter().map(|b| b.name).collect();
    assert_eq!(names.len(), 3, "保留 3 份：{names:?}");
    assert!(!names.contains(&"b0".to_string()), "最舊的被刪掉");
    assert!(names.contains(&"b4".to_string()), "最新的留著");
}

#[test]
fn settings_have_cc_switch_defaults() {
    let (dir, conn) = tmp_db();
    assert_eq!(interval_hours(&conn), 24, "預設 24 小時（與 cc-switch 相同）");
    assert_eq!(retention(&conn), 10, "預設保留 10 份");
    // 亂填的值回預設
    crate::db::set_setting(&conn, "db_backup_interval", "13").unwrap();
    assert_eq!(interval_hours(&conn), 24);
    crate::db::set_setting(&conn, "db_backup_interval", "6").unwrap();
    assert_eq!(interval_hours(&conn), 6);
    crate::db::set_setting(&conn, "db_backup_keep", "999").unwrap();
    assert_eq!(retention(&conn), 10);
    crate::db::set_setting(&conn, "db_backup_keep", "50").unwrap();
    assert_eq!(retention(&conn), 50);

    let st = state(&conn, dir.path());
    assert_eq!(st.interval_options, INTERVAL_OPTIONS.to_vec());
    assert_eq!(st.retention_options, RETENTION_OPTIONS.to_vec());
    assert!(st.dir.contains("db-backups"));
    assert_eq!(st.last_ms, 0, "還沒有備份");
    create(&conn, dir.path(), None).unwrap();
    assert!(state(&conn, dir.path()).last_ms > 0);
}
