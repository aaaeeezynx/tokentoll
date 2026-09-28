//! trace 模組單元測試。

use super::*;
use crate::db::open_and_ensure;
use rusqlite::Connection;

// ─────────────────────────────────────────────────────── 單測 ───

fn mem() -> Connection {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let c = open_and_ensure(&path).unwrap();
    std::mem::forget(dir); // 連線持有期間不刪檔
    c
}

#[test]
fn hex_prefix_truncates_to_max() {
    let small: Vec<u8> = (0u8..=255).collect();
    assert_eq!(hex_prefix(&small, 4), "00010203");
    // 不足 max → 全取（256 bytes → 512 hex 字元）
    assert_eq!(hex_prefix(&small, 4096).len(), 256 * 2);
    // 超過 max → 截斷
    let big = vec![0xabu8; BODY_HEX_MAX + 10];
    assert_eq!(hex_prefix(&big, BODY_HEX_MAX).len(), BODY_HEX_MAX * 2);
    assert!(hex_prefix(&big, 8).chars().all(|c| c == 'a' || c == 'b'));
    assert_eq!(hex_prefix(b"", 8), "");
}

#[test]
fn sha256_of_bytes_matches_known_vector() {
    // SHA-256("abc") 標準測試向量
    assert_eq!(
        sha256_hex_bytes(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn record_helpers_fill_expected_fields() {
    let r = TraceRecord::default()
        .with_body(b"{\"a\":1}")
        .with_upstream_error("boom");
    assert_eq!(r.content_length, Some(7));
    assert_eq!(r.body_sha256.len(), 64);
    assert!(r.body_hex.is_empty(), "with_body 不應記 hex（保護隱私）");
    assert_eq!(r.level, TraceLevel::Warn, "上游錯誤應升為 warn");
    assert_eq!(r.upstream_error, "boom");
}

#[test]
fn with_body_hex_truncates_and_keeps_fingerprint() {
    let body = vec![b'x'; BODY_HEX_MAX + 100];
    let r = TraceRecord::default().with_body_hex(&body);
    assert_eq!(r.body_hex.len(), BODY_HEX_MAX * 2);
    assert_eq!(r.content_length, Some((BODY_HEX_MAX + 100) as i64));
    assert_eq!(r.body_sha256.len(), 64);
}

#[test]
fn stripped_memory_persists_across_connections() {
    // 這是 B2 的回歸測試：記憶必須活過「連線重建」（等同網關重啟）。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    {
        let c = open_and_ensure(&path).unwrap();
        remember_stripped(&c, 7, "prompt_cache_key").unwrap();
        remember_stripped(&c, 7, "verbosity").unwrap();
    }
    let c2 = open_and_ensure(&path).unwrap();
    assert_eq!(
        load_stripped(&c2, 7),
        vec!["prompt_cache_key".to_string(), "verbosity".to_string()],
        "重開連線後仍應記得（原實作存在進程記憶體，重啟即歸零）"
    );
    // 別的渠道不受影響
    assert!(load_stripped(&c2, 8).is_empty());
}

#[test]
fn remember_stripped_is_idempotent() {
    let c = mem();
    for _ in 0..5 {
        remember_stripped(&c, 1, "verbosity").unwrap();
    }
    assert_eq!(load_stripped(&c, 1).len(), 1);
    // 空字串不記
    remember_stripped(&c, 1, "").unwrap();
    assert_eq!(load_stripped(&c, 1).len(), 1);
}

#[test]
fn clear_stripped_only_affects_one_provider() {
    let c = mem();
    remember_stripped(&c, 1, "a").unwrap();
    remember_stripped(&c, 2, "b").unwrap();
    assert_eq!(clear_stripped(&c, 1).unwrap(), 1);
    assert!(load_stripped(&c, 1).is_empty());
    assert_eq!(load_stripped(&c, 2), vec!["b".to_string()]);
}

#[test]
fn stripped_field_summary_counts_distinct_providers() {
    let c = mem();
    remember_stripped(&c, 1, "prompt_cache_key").unwrap();
    remember_stripped(&c, 2, "prompt_cache_key").unwrap();
    remember_stripped(&c, 2, "verbosity").unwrap();
    let sum = stripped_field_summary(&c);
    assert_eq!(
        sum,
        vec![("prompt_cache_key".to_string(), 2), ("verbosity".to_string(), 1)]
    );
}

#[test]
fn trace_roundtrip_via_connection() {
    let c = mem();
    let rec = TraceRecord {
        app: "codex".into(),
        model_raw: "gpt-5".into(),
        in_fmt: "responses".into(),
        target_fmt: "openai-chat".into(),
        trans_kind: "responses_to_chat".into(),
        upstream_status: 400,
        latency_ms: 123,
        retry_count: 1,
        stripped_fields: vec!["prompt_cache_key".into()],
        content_type: "application/json".into(),
        ..Default::default()
    }
    .with_body(b"hello")
    .with_upstream_error("Unsupported parameter(s): `prompt_cache_key`");
    let id = insert_trace(&c, &rec).unwrap();
    assert!(id > 0);

    let got = list_recent(&c, 10).unwrap();
    assert_eq!(got.len(), 1);
    let g = &got[0];
    assert_eq!(g.id, id);
    assert_eq!(g.app, "codex");
    assert_eq!(g.level, "warn");
    assert_eq!(g.upstream_status, 400);
    assert_eq!(g.retry_count, 1);
    assert_eq!(g.stripped_fields, vec!["prompt_cache_key".to_string()]);
    assert_eq!(g.content_length, Some(5));
    assert_eq!(g.body_sha256, sha256_hex_bytes(b"hello"));
    assert!(g.body_hex.is_empty());
    assert!(g.upstream_error.contains("Unsupported parameter"));
}

#[test]
fn list_problems_filters_out_clean_requests() {
    let c = mem();
    // 乾淨請求：info、無剝離、無重試 → 不應出現
    insert_trace(
        &c,
        &TraceRecord {
            app: "claude".into(),
            upstream_status: 200,
            ..Default::default()
        },
    )
    .unwrap();
    // 有剝離 → 應出現
    insert_trace(
        &c,
        &TraceRecord {
            stripped_fields: vec!["verbosity".into()],
            upstream_status: 200,
            ..Default::default()
        },
    )
    .unwrap();
    // warn → 應出現
    insert_trace(
        &c,
        &TraceRecord::default().with_upstream_error("bad"),
    )
    .unwrap();
    // 有重試 → 應出現
    insert_trace(
        &c,
        &TraceRecord {
            retry_count: 1,
            upstream_status: 200,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(list_recent(&c, 10).unwrap().len(), 4);
    assert_eq!(list_problems(&c, 10).unwrap().len(), 3);
}

#[test]
fn body_hex_persists_for_parse_failure_diagnosis() {
    // §5.2 的關鍵需求：解析失敗時必須留下原始位元組的 hex。
    let c = mem();
    let raw = b"{\"model\":\"x\"}";
    insert_trace(
        &c,
        &TraceRecord {
            in_fmt: "anthropic".into(),
            upstream_status: 400,
            note: "body 解析失敗".into(),
            ..Default::default()
        }
        .with_body_hex(raw),
    )
    .unwrap();
    let g = &list_recent(&c, 1).unwrap()[0];
    assert_eq!(g.body_hex, hex_prefix(raw, BODY_HEX_MAX));
    assert_eq!(g.content_length, Some(raw.len() as i64));
    assert_eq!(g.note, "body 解析失敗");
}

#[test]
fn prune_and_clear_work() {
    let c = mem();
    for i in 0..10 {
        insert_trace(
            &c,
            &TraceRecord {
                app: format!("app{i}"),
                ..Default::default()
            },
        )
        .unwrap();
    }
    assert_eq!(prune_traces(&c, 3).unwrap(), 7);
    assert_eq!(list_recent(&c, 100).unwrap().len(), 3);
    assert_eq!(clear_traces(&c).unwrap(), 3);
    assert!(list_recent(&c, 100).unwrap().is_empty());
}

#[test]
fn list_recent_clamps_limit() {
    let c = mem();
    // 不應 panic，且 limit 被夾在合法範圍
    assert!(list_recent(&c, 0).is_ok());
    assert!(list_recent(&c, 99_999).is_ok());
}
