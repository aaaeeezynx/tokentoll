//! `price_extract.rs` 的測試（由原檔尾端搬出，程式碼未改）。

use super::*;

#[test]
fn html_strips_script_and_tags() {
    let html = r#"<html><head><script>var x=1</script><style>.a{}</style></head><body><h1>DeepSeek $0.14/M</h1><p>output $0.28</p></body></html>"#;
    let t = html_to_text(html);
    assert!(t.contains("DeepSeek"));
    assert!(!t.contains("var x"));
    assert!(!t.contains('<'));
}

#[test]
fn json_slice_extracts_first_object() {
    let s = "好的```json\n{\"in_pm\": 1.0, \"out_pm\": 2.0}\n```結束";
    assert_eq!(extract_json_slice(s).unwrap(), "{\"in_pm\": 1.0, \"out_pm\": 2.0}");
    assert!(extract_json_slice("no json here").is_err());
}

#[test]
fn validate_rejects_negative() {
    let p = LlmPrice { in_pm: -1.0, out_pm: 0.0, cache_read_pm: None, cache_create_pm: None, currency: None, confidence: None, evidence: None };
    assert!(validate_price(p, "m", "").is_err());
}

#[test]
fn validate_defaults_cache_zero() {
    let p = LlmPrice { in_pm: 0.14, out_pm: 0.28, cache_read_pm: None, cache_create_pm: None, currency: Some("USD".into()), confidence: Some(0.9), evidence: Some("input $0.14".into()) };
    let r = validate_price(p, "deepseek-v4-flash", "note").unwrap();
    assert_eq!(r.cache_read_pm, 0.0);
    assert_eq!(r.used_model, "deepseek-v4-flash");
}

#[test]
fn fetch_rejects_non_http() {
    assert!(fetch_url_text("ftp://x").is_err());
}

#[test]
fn non_openai_format_rejected_before_network() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    conn.execute(
        "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme, models_json, priority, enabled, created_at, updated_at)
         VALUES ('gem-src','universal','gemini','http://x','k','bearer','[]',0,1,0,0)",
        [],
    )
    .unwrap();
    let pid: i64 = conn.query_row("SELECT id FROM providers WHERE name='gem-src'", [], |r| r.get(0)).unwrap();
    let err = price_extract(&conn, pid, "m", None, Some("input $1"), None, None).unwrap_err();
    assert!(err.contains("OpenAI"), "實際：{err}");
}
