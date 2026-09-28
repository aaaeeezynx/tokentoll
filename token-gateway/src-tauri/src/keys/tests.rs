//! `keys` 模組的測試（由 `keys/mod.rs` 尾端搬出，程式碼未改）。

use super::*;

#[test]
fn secret_roundtrip_and_prefix() {
    let s1 = gen_secret();
    let s2 = gen_secret();
    assert!(s1.starts_with(KEY_PREFIX));
    assert_ne!(s1, s2);
    assert_ne!(sha256_hex(&s1), s1, "存的必須是哈希");
    assert_eq!(sha256_hex(&s1).len(), 64);
}

fn test_input() -> KeyInput {
    KeyInput {
        name: "k".into(),
        provider_id: None,
        quota_tokens: -1,
        rate_limit_qpm: 0,
        allowed_models: vec![],
        allowed_apps: vec![],
        expires_at: None,
    }
}

#[test]
fn direct_matches_provider_key() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
         models_json, priority, enabled, created_at, updated_at)
         VALUES ('d1','universal','openai-chat','http://up','up-secret','bearer','[]',0,1,0,0)",
        [],
    )
    .unwrap();
    let a = auth_direct(&conn, "up-secret").unwrap();
    assert!(a.direct);
    assert_eq!(a.id, -1);
    assert_eq!(a.provider_api_key, "up-secret");
    assert_eq!(a.provider_base_url, "http://up");
    assert!(auth_direct(&conn, "nope").is_err());
    assert!(auth_direct(&conn, "").is_err());
    assert!(auth_direct(&conn, "   ").is_err());
}

#[test]
fn rotate_replaces_secret() {        let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    let created = create_key(&conn, &test_input()).unwrap();
    assert!(auth_key(&conn, &created.secret).is_err(), "未綁定渠道應 500");
    let r2 = rotate_key(&conn, created.id).unwrap();
    assert_eq!(r2.id, created.id);
    assert_ne!(r2.secret, created.secret);
    assert!(auth_key(&conn, &created.secret).is_err(), "舊 Key 必須失效");
    assert!(rotate_key(&conn, 999999).is_err());
}
