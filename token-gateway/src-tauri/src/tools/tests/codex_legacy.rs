//! Codex 舊會話 provider 的讀取與別名管理（B3 迴歸）。
//!
//! 由 `tools/tests/codex.rs` 再拆出來（該檔 2026-09-28 時 432 行 > 400）。
//! 測試內容與斷言一字未改；共用 helper 由父模組 `tools::tests` 提供。

use super::*;

#[test]
fn codex_legacy_providers_reads_threads() {
    // 自造最小 state_5.sqlite：去重 custom/tokengateway，丟棄非法名
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state_5.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE threads (id TEXT, model_provider TEXT NOT NULL);
         INSERT INTO threads VALUES ('1','custom'),('2','nim-direct'),('3','nim-direct'),
         ('4','tokengateway'),('5','opencode-zen'),('6','bad key!'),('7','');",
    )
    .unwrap();
    drop(conn);
    match codex_legacy_providers_report(dir.path()) {
        LegacyProviders::Ok { providers, .. } => assert_eq!(
            providers,
            vec!["nim-direct".to_string(), "opencode-zen".to_string()]
        ),
        LegacyProviders::Failed { reason } => panic!("應該讀得到：{reason}"),
    }
}

/// 迴歸測試：state DB 的檔名**帶版本後綴**（state_5 / state_7 …），
/// 不可寫死。Codex 升版把它換掉時，寫死的程式會靜默回傳空 vec，
/// 別名段全消失、舊會話失去 provider。
#[test]
fn codex_state_db_follows_versioned_filename() {
    let dir = tempfile::tempdir().unwrap();
    for (name, prov) in [
        ("state_5.sqlite", "old-prov"),
        ("state_7.sqlite", "new-prov"),
    ] {
        let conn = rusqlite::Connection::open(dir.path().join(name)).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE threads (id TEXT, model_provider TEXT NOT NULL);
             INSERT INTO threads VALUES ('1','{prov}');"
        ))
        .unwrap();
    }
    match codex_legacy_providers_report(dir.path()) {
        LegacyProviders::Ok { providers, db } => {
            // 版號高者優先
            assert_eq!(providers, vec!["new-prov".to_string()]);
            assert!(
                db.to_string_lossy().ends_with("state_7.sqlite"),
                "應選 state_7，實際 {db:?}"
            );
        }
        LegacyProviders::Failed { reason } => panic!("應該讀得到：{reason}"),
    }
}

/// 迴歸測試：`codex_doctor` 曾把**讀取失敗**報成
/// 「✅ 歷史會話無第三方 provider 殘留」。讀不到必須是 `Failed`，
/// 否則唯一的診斷工具會告訴使用者一切正常。
#[test]
fn codex_legacy_read_failure_is_not_reported_as_empty() {
    // ① 完全沒有 state DB
    let dir = tempfile::tempdir().unwrap();
    match codex_legacy_providers_report(dir.path()) {
        LegacyProviders::Failed { reason } => {
            assert!(reason.contains("state*.sqlite"), "原因應說明找不到檔案：{reason}")
        }
        LegacyProviders::Ok { providers, .. } => {
            panic!("沒有任何 state DB 應該是 Failed，不是 Ok（{providers:?}）")
        }
    }
    // ② 有 state DB 但缺 threads 表（schema 變動）
    let conn = rusqlite::Connection::open(dir.path().join("state_9.sqlite")).unwrap();
    conn.execute_batch("CREATE TABLE something_else (x TEXT);")
        .unwrap();
    drop(conn);
    match codex_legacy_providers_report(dir.path()) {
        LegacyProviders::Failed { reason } => {
            assert!(reason.contains("threads"), "原因應提到 threads：{reason}")
        }
        LegacyProviders::Ok { providers, .. } => {
            panic!("缺 threads 表應該是 Failed，不是 Ok（{providers:?}）")
        }
    }
}

/// 迴歸測試：既有的網關別名段不可因為 threads 變少而被剔除。
/// 使用者**封存對話**後，那個 provider 可能不再出現在 threads 裡；
/// 若管理清單只來自 DB，`codex_apply` 就會把別名段刪掉，舊會話失去
/// provider 而無法續用。
#[test]
fn codex_alias_ids_never_shrinks_existing_gateway_aliases() {
    let gw = "http://127.0.0.1:15722/v1";
    let existing = format!(
        "model = \"m\"\n\
         [model_providers.{CODEX_SHARED_PROVIDER_ID}]\nbase_url = \"{gw}\"\n\
         [model_providers.{GATEWAY_PROVIDER_ID}]\nbase_url = \"{gw}\"\n\
         [model_providers.oldalias]\nbase_url = \"{gw}\"\n\
         [model_providers.foreign]\nbase_url = \"https://api.example.com/v1\"\n"
    );
    // DB 讀到空（＝threads 已不含任何第三方 provider）
    let got = codex_alias_ids(&existing, &[], gw);
    assert!(
        got.contains(&"oldalias".to_string()),
        "既有網關別名段必須保留：{got:?}"
    );
    assert!(!got.contains(&CODEX_SHARED_PROVIDER_ID.to_string()));
    assert!(!got.contains(&GATEWAY_PROVIDER_ID.to_string()));
    assert!(
        !got.contains(&"foreign".to_string()),
        "指向上游的 provider 不該被劫持：{got:?}"
    );
}

/// DB 讀到的名稱要保留，與既有段聯集時不重複。
#[test]
fn codex_alias_ids_unions_db_and_existing() {
    let gw = "http://127.0.0.1:15722/v1";
    let existing = format!("[model_providers.oldalias]\nbase_url = \"{gw}\"\n");
    let got = codex_alias_ids(
        &existing,
        &["fromdb".to_string(), "oldalias".to_string()],
        gw,
    );
    assert!(got.contains(&"fromdb".to_string()));
    assert_eq!(
        got.iter().filter(|x| *x == "oldalias").count(),
        1,
        "不應重複：{got:?}"
    );
}
