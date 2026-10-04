//! 各工具設定寫入與計畫驗證
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;


#[test]
fn plan_claude_model_map_preview() {
    let req = SwitchRequest {
        app: "claude".into(),
        base_url: "http://127.0.0.1:15722".into(),
        api_key: "sk-local-x".into(),
        model: "m".into(),
        provider_id: None,
        provider_format: None,
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: false,
        key_id: None,
        claude_map: Some(ClaudeModelMap {
            default: "a".into(),
            haiku: "b".into(),
            sonnet: "".into(),
            opus: "c".into(),
            fable: "d".into(),
        }),
    };
    let plan = plan_switch(&req, 15722).unwrap();
    assert!(plan.edits.iter().any(|e| e.contains("ANTHROPIC_MODEL = a")), "{plan:?}");
    assert!(plan.edits.iter().any(|e| e.contains("ANTHROPIC_DEFAULT_HAIKU_MODEL = b")), "{plan:?}");
    assert!(plan.edits.iter().any(|e| e.contains("ANTHROPIC_DEFAULT_OPUS_MODEL = c")), "{plan:?}");
    assert!(plan.edits.iter().any(|e| e.contains("ANTHROPIC_DEFAULT_FABLE_MODEL = d")), "{plan:?}");
    assert!(!plan.edits.iter().any(|e| e.contains("SONNET")), "空檔位不預覽：{plan:?}");
}


#[test]
fn claude_merges_env_and_keeps_other_keys() {
    let old = r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:15721", "ANTHROPIC_DEFAULT_SONNET_MODEL": "x"}, "other": 1}"#;
    let out = claude_apply(Some(old), "http://127.0.0.1:15722", "sk-test", None, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:15722");
    assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "x");
    assert_eq!(v["other"], 1);
}


#[test]
fn claude_creates_file_from_scratch() {
    let out = claude_apply(None, "http://127.0.0.1:15721", "sk-a", None, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-a");
}


#[test]
fn claude_model_map_writes_four_tiers() {
    let map = ClaudeModelMap {
        default: "deepseek-v4-flash".into(),
        haiku: "deepseek-v4-flash".into(),
        sonnet: "deepseek-v4-flash".into(),
        opus: "muse-spark-1.3-contributor".into(),
        fable: "mimo-v2.6-flash".into(),
    };
    let out = claude_apply(None, "http://127.0.0.1:15722", "sk-test", Some(&map), None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["env"]["ANTHROPIC_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"], "muse-spark-1.3-contributor");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_FABLE_MODEL"], "mimo-v2.6-flash");
    // 空檔位不覆蓋現值
    let old = r#"{"env": {"ANTHROPIC_DEFAULT_HAIKU_MODEL": "keep"}}"#;
    let empty = ClaudeModelMap::default();
    let out2 = claude_apply(Some(old), "http://127.0.0.1:15722", "sk-test", Some(&empty), None).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    assert_eq!(v2["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "keep");
    assert!(v2["env"].get("ANTHROPIC_MODEL").is_none());
}


#[test]
fn claude_writes_context_override_only_when_known() {
    // Claude Code 對它不認識的模型只假設 200k，所以知道真實視窗時要告訴它；
    // 不知道就**不寫** —— 不編數字。
    let map = ClaudeModelMap {
        default: "deepseek-v4.1-flash".into(),
        ..Default::default()
    };
    let known = claude_apply(
        None,
        "http://127.0.0.1:15722",
        "sk-test",
        Some(&map),
        Some(1_000_000),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&known).unwrap();
    assert_eq!(v["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "1000000");
    assert_eq!(v["env"]["ANTHROPIC_MODEL"], "deepseek-v4.1-flash");

    for unknown in [None, Some(0), Some(-1)] {
        let out =
            claude_apply(None, "http://127.0.0.1:15722", "sk-test", Some(&map), unknown).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(
            v["env"].get("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_none(),
            "{unknown:?} 不該寫入視窗上限"
        );
    }
}


#[test]
fn opencode_merges_provider_and_models() {
    let old = r#"{"$schema": "x", "provider": {"anthropic": {"options": {}}}, "model": "anthropic/claude"}"#;
    let out = opencode_apply(Some(old), "http://127.0.0.1:15721/v1", "deepseek-v4-flash", None, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["provider"]["tokentoll"]["options"]["baseURL"],
        "http://127.0.0.1:15721/v1"
    );
    assert_eq!(
        v["provider"]["tokentoll"]["options"]["apiKey"],
        "{env:TOKEN_TOLL_KEY}"
    );
    assert_eq!(
        v["provider"]["tokentoll"]["npm"],
        "@ai-sdk/openai-compatible"
    );
    assert!(v["provider"]["tokentoll"]["models"]["deepseek-v4-flash"].is_object());
    assert!(v["provider"]["anthropic"].is_object(), "原有渠道必須保留");
    // 接管時釘住模型：否則 `opencode run` 會用它自己的預設模型，請求不落到網關
    assert_eq!(
        v["model"], "tokentoll/deepseek-v4-flash",
        "頂層 model 必須指向網關 provider"
    );
}


#[test]
fn opencode_pins_model_to_gateway_provider() {
    // 使用者原本的 model（別的 provider）被接管覆蓋，且格式是 <provider>/<model>
    let old = r#"{"provider": {}, "model": "anthropic/claude-sonnet-4-6"}"#;
    let out = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "muse-spark-1.3", None, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["model"], "tokentoll/muse-spark-1.3");
    // 空模型不得寫出 `tokentoll/`（否則會釘到一個不存在的模型）
    let out2 = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "   ", None, None).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    assert_eq!(v2["model"], "anthropic/claude-sonnet-4-6", "無模型時不動原值");
}


#[test]
fn opencode_writes_context_limit() {
    let old = r#"{"provider": {}}"#;
    let out = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "m1", Some(128000), None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["provider"]["tokentoll"]["models"]["m1"]["limit"]["context"],
        128000
    );
    let out2 = opencode_apply(Some(old), "http://x", "m1", None, None).unwrap();
    assert!(out2.contains("\"m1\""), "{out2}");
    assert!(!out2.contains("limit"), "未設定時不寫 limit：{out2}");
}


#[test]
fn opencode_direct_key_writes_plaintext() {
    let old = r#"{"provider": {}}"#;
    // 直連模式：注入的上游 Key 明文寫入，不再是 {env:} 引用
    let out = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "m1", None, Some("up-secret")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["provider"]["tokentoll"]["options"]["apiKey"],
        "up-secret"
    );
    // 空 Key 回退引用（不寫空字串，避免靜默斷連）
    let out2 = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "m1", None, Some("  ")).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    assert_eq!(
        v2["provider"]["tokentoll"]["options"]["apiKey"],
        "{env:TOKEN_TOLL_KEY}"
    );
}


#[test]
fn plan_rejects_unknown_app_and_empty_model() {
    let req = SwitchRequest {
        app: "nope".into(),
        base_url: "http://x".into(),
        api_key: "k".into(),
        model: "m".into(),
        provider_id: None,
        provider_format: None,
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: false,
        key_id: None,
        claude_map: None,
    };
    assert!(plan_switch(&req, 15721).is_err());
    let mut req2 = req;
    req2.app = "codex".into();
    req2.model = "  ".into();
    assert!(plan_switch(&req2, 15721).is_err());
}
