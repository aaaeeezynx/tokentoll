//! `providers_io` 的回歸測試：複製、匯出、匯入。

use super::*;

/// 空來源庫（`open_and_ensure` 會種入 seed 來源，這裡的計數斷言需從空表起步）。
fn empty_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    for t in [
        "provider_models",
        "provider_pricing",
        "pricing_periods",
        "providers",
    ] {
        conn.execute(&format!("DELETE FROM {t}"), []).unwrap();
    }
    (dir, conn)
}

fn add_provider(conn: &Connection, name: &str, base_url: &str) -> i64 {
    let pr = next_priority(conn).unwrap();
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                models_json, priority, enabled, created_at, updated_at)
         VALUES (?1,'universal','openai-chat',?2,'sk-secret','bearer','[\"inline-a\"]',?3,1,1,1)",
        params![name, base_url, pr],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn add_model(conn: &Connection, pid: i64, name: &str, cw: Option<i64>) {
    conn.execute(
        "INSERT INTO provider_models (provider_id, display_name, actual_model, context_window,
                                      reasoning, ord, enabled)
         VALUES (?1,?2,?2,?3,'high',0,1)",
        params![pid, name, cw],
    )
    .unwrap();
}

fn add_pricing(conn: &Connection, pid: i64, model: &str) {
    conn.execute(
        "INSERT INTO provider_pricing (provider_id, model_norm, mode, in_pm, out_pm,
                                       cache_read_pm, cache_create_pm, sub_fee_usd,
                                       sub_included_tokens, source)
         VALUES (?1,?2,'usage',0.5,1.5,0.05,0.06,0,0,'custom')",
        params![pid, model],
    )
    .unwrap();
}

fn add_period(conn: &Connection, pid: i64) {
    conn.execute(
        "INSERT INTO pricing_periods (provider_id, model_norm, name, start, end, ord,
                                      in_pm, out_pm, cache_read_pm, cache_create_pm)
         VALUES (?1,'*','夜間','23:00','07:00',0,0.2,0.6,0,0)",
        [pid],
    )
    .unwrap();
}

fn count(conn: &Connection, sql: &str, pid: i64) -> i64 {
    conn.query_row(sql, [pid], |r| r.get(0)).unwrap()
}

fn order(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM providers ORDER BY priority ASC, id ASC")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

/// 複製要帶走「來源的全部內容」，而且插在原本那筆後面。
#[test]
fn duplicate_copies_everything_and_inserts_after_original() {
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "oc-go", "https://opencode.ai/zen/go/v1");
    let b = add_provider(&conn, "backup", "https://example.com/v1");
    add_model(&conn, a, "grok-4.7", Some(1_000_000));
    add_model(&conn, a, "mimo-v2.6-pro", None);
    add_pricing(&conn, a, "grok-4.7");
    add_period(&conn, a);

    let a2 = duplicate_provider(&conn, a).unwrap();
    assert_ne!(a, a2);
    assert_eq!(
        order(&conn),
        vec!["oc-go", "oc-go 複製", "backup"],
        "複製品要緊跟在原本那筆後面"
    );
    // 原本在後面那筆的 priority 要被往後推一格（插在中間不撞號）
    let pb: i64 = conn
        .query_row("SELECT priority FROM providers WHERE id=?1", [b], |r| r.get(0))
        .unwrap();
    assert_eq!(pb, 2);
    // 連線資訊與 inline models 原樣帶走
    let (url, key, mj): (String, String, String) = conn
        .query_row(
            "SELECT base_url, api_key, models_json FROM providers WHERE id=?1",
            [a2],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(url, "https://opencode.ai/zen/go/v1");
    assert_eq!(key, "sk-secret");
    assert_eq!(mj, "[\"inline-a\"]");
    // 模型（含 context_window）、每模型價格、訂閱時段都要一起過去
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM provider_models WHERE provider_id=?1", a2),
        2
    );
    let cw: Option<i64> = conn
        .query_row(
            "SELECT context_window FROM provider_models WHERE provider_id=?1 AND display_name='grok-4.7'",
            [a2],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cw, Some(1_000_000), "context_window 必須一起複製");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM provider_pricing WHERE provider_id=?1", a2),
        1
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM pricing_periods WHERE provider_id=?1", a2),
        1
    );
    // 再複製一次：序號往上加，不會撞名
    let a3 = duplicate_provider(&conn, a).unwrap();
    let n3: String = conn
        .query_row("SELECT name FROM providers WHERE id=?1", [a3], |r| r.get(0))
        .unwrap();
    assert_eq!(n3, "oc-go 複製 2");
    // 原本那筆不能被改動
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM provider_models WHERE provider_id=?1", a),
        2
    );
}

/// 複製**不該**帶走金鑰綁定與學到的行為。
#[test]
fn duplicate_leaves_keys_and_learned_state_alone() {
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "src", "https://example.com/v1");
    conn.execute(
        "INSERT INTO provider_stripped_fields (provider_id, field, learned_at) VALUES (?1,'foo',1)",
        [a],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO provider_model_protocol (provider_id, model, declared_format, actual_format, learned_at)
         VALUES (?1,'m','openai-chat','openai-responses',1)",
        [a],
    )
    .unwrap();
    let a2 = duplicate_provider(&conn, a).unwrap();
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM provider_stripped_fields WHERE provider_id=?1", a2),
        0
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM provider_model_protocol WHERE provider_id=?1", a2),
        0
    );
}

/// 預設匯出不含金鑰；要求含金鑰時才帶出去，而且檔頭寫明。
#[test]
fn export_hides_keys_unless_explicitly_asked() {
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "src", "https://example.com/v1");
    add_model(&conn, a, "m1", Some(200_000));
    add_pricing(&conn, a, "m1");
    add_period(&conn, a);

    let safe = export_providers(&conn, false).unwrap();
    assert!(!safe.includes_keys);
    assert_eq!(safe.providers.len(), 1);
    assert_eq!(safe.providers[0].api_key, "", "預設不得帶出金鑰");
    assert_eq!(safe.providers[0].models.len(), 1);
    assert_eq!(safe.providers[0].pricing.len(), 1);
    assert_eq!(safe.providers[0].periods.len(), 1);
    assert_eq!(safe.kind, "token-gateway/providers");
    assert_eq!(safe.version, EXPORT_VERSION);

    let with = export_providers(&conn, true).unwrap();
    assert!(with.includes_keys);
    assert_eq!(with.providers[0].api_key, "sk-secret");
}

/// 匯入是 skip 語意：同 `(name, base_url)` 已存在就跳過，不覆蓋。
#[test]
fn import_skips_existing_pairs() {
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "src", "https://example.com/v1");
    add_model(&conn, a, "m1", None);
    let file = export_providers(&conn, false).unwrap();

    // 匯入自己的匯出檔 → 全部跳過
    let rep = import_providers(&conn, &file).unwrap();
    assert_eq!((rep.added, rep.skipped), (0, 1));
    assert_eq!(order(&conn).len(), 1, "不得多出重複來源");

    // 同名但不同 base_url → 視為不同來源，要新增（名稱自動讓開，避免兩張同名卡）
    let mut other = file.clone();
    other.providers[0].base_url = "https://other.example.com/v1".into();
    let rep = import_providers(&conn, &other).unwrap();
    assert_eq!((rep.added, rep.skipped), (1, 0));
    assert_eq!(order(&conn), vec!["src".to_string(), "src 2".to_string()]);
}

/// 匯入到空庫：內容要完整還原（模型／價格／時段），名稱撞名要自動讓開。
#[test]
fn import_restores_content_into_empty_db() {
    let (_d, src) = empty_db();
    let p = add_provider(&src, "oc-go", "https://opencode.ai/zen/go/v1");
    add_model(&src, p, "grok-4.7", Some(1_000_000));
    add_model(&src, p, "mimo-v2.6-pro", None);
    add_pricing(&src, p, "grok-4.7");
    add_period(&src, p);
    let file = export_providers(&src, false).unwrap();

    let (_d2, dst) = empty_db();
    // 先放一筆同名不同網址的來源，逼出「同名」路徑
    add_provider(&dst, "oc-go", "https://elsewhere.example.com/v1");
    let rep = import_providers(&dst, &file).unwrap();
    assert_eq!(rep.added, 1);
    assert_eq!(rep.models, 2);
    assert_eq!(rep.pricing, 1);
    assert_eq!(rep.periods, 1);
    assert_eq!(rep.names, vec!["oc-go 2".to_string()], "撞名要自動讓開");
    let new_id: i64 = conn_id_by_name(&dst, "oc-go 2");
    assert_eq!(
        count(&dst, "SELECT COUNT(*) FROM provider_models WHERE provider_id=?1", new_id),
        2
    );
    assert_eq!(
        count(&dst, "SELECT COUNT(*) FROM pricing_periods WHERE provider_id=?1", new_id),
        1
    );
    // 不含金鑰的匯出檔匯入後，金鑰必須是空的（不能沿用別人家的）
    let key: String = dst
        .query_row("SELECT api_key FROM providers WHERE id=?1", [new_id], |r| r.get(0))
        .unwrap();
    assert_eq!(key, "");
}

/// 匯出 → 清空 → 匯入：內容一致（roundtrip）。
#[test]
fn export_then_import_roundtrips() {
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "A", "https://a.example.com/v1");
    let b = add_provider(&conn, "B", "https://b.example.com/v1");
    add_model(&conn, a, "ma", Some(100));
    add_model(&conn, b, "mb", Some(200));
    add_pricing(&conn, b, "mb");
    let before = export_providers(&conn, true).unwrap();

    conn.execute("DELETE FROM provider_models", []).unwrap();
    conn.execute("DELETE FROM pricing_periods", []).unwrap();
    conn.execute("DELETE FROM provider_pricing", []).unwrap();
    conn.execute("DELETE FROM providers", []).unwrap();

    let rep = import_providers(&conn, &before).unwrap();
    assert_eq!(rep.added, 2);
    let after = export_providers(&conn, true).unwrap();
    assert_eq!(after.providers.len(), before.providers.len());
    for (x, y) in before.providers.iter().zip(after.providers.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.base_url, y.base_url);
        assert_eq!(x.api_key, y.api_key, "含金鑰匯出要能還原金鑰");
        assert_eq!(x.models.len(), y.models.len());
        assert_eq!(x.pricing.len(), y.pricing.len());
        assert_eq!(x.periods.len(), y.periods.len());
    }
}

/// 丟錯檔案／版本太新要給得出人話，而不是靜默吞掉。
#[test]
fn import_rejects_foreign_or_future_files() {
    let (_d, conn) = empty_db();
    let mut file = export_providers(&conn, false).unwrap();
    file.kind = "something-else".into();
    let e = import_providers(&conn, &file).unwrap_err();
    assert!(e.contains("不是來源匯出檔"), "{e}");

    let mut file = export_providers(&conn, false).unwrap();
    file.version = EXPORT_VERSION + 1;
    let e = import_providers(&conn, &file).unwrap_err();
    assert!(e.contains("版本太新"), "{e}");
}

/// JSON 解析：缺欄位要有預設值（前端只會傳必要的東西）。
#[test]
fn import_parses_minimal_json() {
    let (_d, conn) = empty_db();
    let json = r#"{
      "kind": "token-gateway/providers",
      "version": 1,
      "providers": [ { "name": "min", "base_url": "https://min.example.com/v1" } ]
    }"#;
    let file: ExportFile = serde_json::from_str(json).unwrap();
    let rep = import_providers(&conn, &file).unwrap();
    assert_eq!(rep.added, 1);
    let (fmt, scheme, enabled): (String, String, i64) = conn
        .query_row(
            "SELECT api_format, auth_scheme, enabled FROM providers WHERE name='min'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((fmt.as_str(), scheme.as_str(), enabled), ("openai-chat", "bearer", 1));
}

// 這個小工具只是為了讓上面的測試好讀。
fn conn_id_by_name(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM providers WHERE name=?1", [name], |r| {
        r.get(0)
    })
    .unwrap()
}

/// 匯出檔案：真的寫到磁碟、可以被匯入讀回來，而且同一秒連續匯出不會互相覆蓋。
///
/// 這條測試的由來是一個實機 bug：原本靠瀏覽器 Blob 下載，第一次可以、第二次
/// WebView2 會跳原生權限對話框蓋住 UI（見 `write_export` 的註解）。
#[test]
fn export_file_writes_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let (_d, conn) = empty_db();
    let a = add_provider(&conn, "src", "https://example.com/v1");
    add_model(&conn, a, "m1", Some(1000));
    let json = serde_json::to_string_pretty(&export_providers(&conn, false).unwrap()).unwrap();

    let p1 = write_export(dir.path(), "20261001-000000", &json).unwrap();
    assert!(p1.exists(), "檔案要真的寫出來");
    assert!(p1.file_name().unwrap().to_str().unwrap().ends_with(".json"));
    // 同一秒再匯出一次 → 另存一個檔，不覆蓋
    let p2 = write_export(dir.path(), "20261001-000000", &json).unwrap();
    assert_ne!(p1, p2);
    assert!(p2.exists() && p1.exists());
    assert!(p2.file_name().unwrap().to_str().unwrap().contains("-2"));

    // 寫出來的檔案要能被「從路徑匯入」那條路讀回（內容可解析且 provider 對得上）
    let raw = std::fs::read_to_string(&p1).unwrap();
    let back: ExportFile = serde_json::from_str(&raw).unwrap();
    assert_eq!(back.kind, "token-gateway/providers");
    assert_eq!(back.providers.len(), 1);
    assert_eq!(back.providers[0].name, "src");
    assert_eq!(back.providers[0].models.len(), 1);
    // 從路徑匯入到空庫：內容要進得去
    let (_d2, dst) = empty_db();
    let rep = import_providers(&dst, &back).unwrap();
    assert_eq!(rep.added, 1);
    assert_eq!(rep.models, 1);
}

/// 匯出目錄不存在時要自己建出來（使用者把 Downloads 指到別的地方的情況）。
#[test]
fn export_file_creates_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    let p = write_export(&nested, "20261001-000000", "{}").unwrap();
    assert!(p.exists());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}");
}
