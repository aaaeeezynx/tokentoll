//! quota 模組單元測試。

use super::*;
use serde_json::json;

#[test]
fn detect_opencode_go_matches() {
    assert!(detect_opencode_go("https://opencode.ai/zen/go/v1"));
    assert!(detect_opencode_go("https://opencode.ai/zen/go"));
    assert!(!detect_opencode_go("https://opencode.ai/zen/v1"));
    assert!(!detect_opencode_go("https://api.openai.com/v1"));
}

#[test]
fn opencode_windows_parsed_and_zero_percent_keeps_reset() {
    let body = json!({"usage": {
        "rolling": {"status": "ok", "percent": 12.5, "resetsAt": "2026-08-13T16:27:38.287Z"},
        "weekly": {"status": "ok", "percent": 0, "resetsAt": "2026-08-17T00:00:00.287Z"},
        "monthly": {"status": "ok", "percent": 3, "resetsAt": "2026-09-13T06:06:01.287Z"}
    }});
    let w = parse_opencode_windows(&body);
    assert_eq!(w.len(), 3);
    assert_eq!(w[0].label, "5 小時");
    assert_eq!(w[0].percent, 12.5);
    assert!(w[0].resets_at.is_some());
    // 零用量也要帶重置時間：上游在 0% 仍回有意義的 resetsAt，後台頁也照樣顯示倒數
    assert_eq!(w[1].percent, 0.0);
    assert_eq!(
        w[1].resets_at.as_deref(),
        Some("2026-08-17T00:00:00.287Z"),
        "0% 的窗口不能丟重置時間"
    );
    assert_eq!(w[2].label, "本月");
    // 上游沒給 resetsAt 就顯示「—」，不要憑空補一個
    let body = json!({"usage": {"rolling": {"status": "ok", "percent": 0}}});
    let w = parse_opencode_windows(&body);
    assert_eq!(w.len(), 1);
    assert!(w[0].resets_at.is_none());
}

#[test]
fn opencode_windows_skip_bad_status_and_clamp() {
    let body = json!({"usage": {
        "rolling": {"status": "error", "percent": 5},
        "weekly": {"status": "ok", "percent": 130},
        "monthly": {}
    }});
    let w = parse_opencode_windows(&body);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].percent, 100.0);
}

#[test]
fn claude_windows_parsed_with_labels() {
    let body = json!({
        "five_hour": {"utilization": 4.0, "resets_at": "2026-08-13T20:00:00Z"},
        "seven_day": {"utilization": 61.2, "resets_at": "2026-08-17T00:00:00Z"},
        "extra_usage": {"is_enabled": false}
    });
    let w = parse_claude_windows(&body);
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].label, "5 小時");
    assert_eq!(w[1].label, "7 天");
    assert_eq!(w[1].percent, 61.2);
    assert_eq!(w[1].resets_at.as_deref(), Some("2026-08-17T00:00:00Z"));
}

#[test]
fn codex_windows_parsed_with_seconds_label() {
    let body = json!({"rate_limit": {
        "primary_window": {"used_percent": 18, "limit_window_seconds": 18000, "reset_at": 1786000000},
        "secondary_window": {"used_percent": 42.5, "limit_window_seconds": 604800, "reset_at": 1786500000}
    }});
    let w = parse_codex_windows(&body);
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].label, "5 小時");
    assert_eq!(w[1].label, "7 天");
    assert_eq!(w[1].percent, 42.5);
    assert!(w[1].resets_at.as_deref().unwrap().starts_with("2026-"));
}

#[test]
fn codex_window_label_fallbacks() {
    assert_eq!(codex_window_label(18000), "5 小時");
    assert_eq!(codex_window_label(2592000), "30 天");
    assert_eq!(codex_window_label(172800), "2 天");
    assert_eq!(codex_window_label(7200), "2 小時");
}

#[test]
fn query_all_skips_disabled_and_non_go() {
    let providers = vec![
        (1, "go".into(), "https://opencode.ai/zen/go/v1".into(), "".into(), false),
        (2, "nim".into(), "https://integrate.api.nvidia.com/v1".into(), "k".into(), true),
    ];
    let out = query_all(&providers);
    // 停用的 Go 來源被跳過；claude/codex 兩條本地憑證查詢始終在列
    assert!(!out.iter().any(|q| q.kind == "opencode-go"));
    assert!(out.iter().any(|q| q.kind == "claude"));
    assert!(out.iter().any(|q| q.kind == "codex"));
}
