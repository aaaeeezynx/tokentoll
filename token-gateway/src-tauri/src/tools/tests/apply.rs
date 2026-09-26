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
        via_gateway: true,
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
    let out = claude_apply(Some(old), "http://127.0.0.1:15722", "sk-test", None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:15722");
    assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-test");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "x");
    assert_eq!(v["other"], 1);
}


#[test]
fn claude_creates_file_from_scratch() {
    let out = claude_apply(None, "http://127.0.0.1:15721", "sk-a", None).unwrap();
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
    let out = claude_apply(None, "http://127.0.0.1:15722", "sk-test", Some(&map)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["env"]["ANTHROPIC_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "deepseek-v4-flash");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"], "muse-spark-1.3-contributor");
    assert_eq!(v["env"]["ANTHROPIC_DEFAULT_FABLE_MODEL"], "mimo-v2.6-flash");
    // 空檔位不覆蓋現值
    let old = r#"{"env": {"ANTHROPIC_DEFAULT_HAIKU_MODEL": "keep"}}"#;
    let empty = ClaudeModelMap::default();
    let out2 = claude_apply(Some(old), "http://127.0.0.1:15722", "sk-test", Some(&empty)).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    assert_eq!(v2["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "keep");
    assert!(v2["env"].get("ANTHROPIC_MODEL").is_none());
}


#[test]
fn opencode_merges_provider_and_models() {
    let old = r#"{"$schema": "x", "provider": {"anthropic": {"options": {}}}, "model": "anthropic/claude"}"#;
    let out = opencode_apply(Some(old), "http://127.0.0.1:15721/v1", "deepseek-v4-flash", None, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["provider"]["tokengateway"]["options"]["baseURL"],
        "http://127.0.0.1:15721/v1"
    );
    assert_eq!(
        v["provider"]["tokengateway"]["options"]["apiKey"],
        "{env:TOKEN_GATEWAY_KEY}"
    );
    assert_eq!(
        v["provider"]["tokengateway"]["npm"],
        "@ai-sdk/openai-compatible"
    );
    assert!(v["provider"]["tokengateway"]["models"]["deepseek-v4-flash"].is_object());
    assert!(v["provider"]["anthropic"].is_object(), "原有渠道必須保留");
    assert_eq!(v["model"], "anthropic/claude", "頂層 model 不動");
}


#[test]
fn opencode_writes_context_limit() {
    let old = r#"{"provider": {}}"#;
    let out = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "m1", Some(128000), None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["provider"]["tokengateway"]["models"]["m1"]["limit"]["context"],
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
        v["provider"]["tokengateway"]["options"]["apiKey"],
        "up-secret"
    );
    // 空 Key 回退引用（不寫空字串，避免靜默斷連）
    let out2 = opencode_apply(Some(old), "http://127.0.0.1:15722/v1", "m1", None, Some("  ")).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    assert_eq!(
        v2["provider"]["tokengateway"]["options"]["apiKey"],
        "{env:TOKEN_GATEWAY_KEY}"
    );
}


#[test]
fn hermes_switches_provider_and_default() {
    let old = "model:\n  default: deepseek-v4-flash\n  provider: flatkey\nproviders:\n  flatkey:\n    base_url: https://router.flatkey.ai/v1\n";
    let out = hermes_apply(old, "http://127.0.0.1:15721/v1", "kimi-k2.5").unwrap();
    assert!(out.contains("  provider: tokengateway"), "{out}");
    assert!(out.contains("  default: kimi-k2.5"), "{out}");
    assert!(out.contains("  tokengateway:\n    name: tokengateway\n    base_url: http://127.0.0.1:15721/v1"), "{out}");
    assert!(out.contains("  flatkey:"), "原有渠道必須保留");
}


#[test]
fn hermes_updates_existing_gateway_block() {
    let old = "providers:\n  tokengateway:\n    name: tokengateway\n    base_url: http://127.0.0.1:9999/v1\nmodel:\n  provider: tokengateway\n  default: a\n";
    let out = hermes_apply(old, "http://127.0.0.1:15721/v1", "b").unwrap();
    assert!(out.contains("    base_url: http://127.0.0.1:15721/v1"), "{out}");
    assert!(!out.contains("9999"), "舊地址必須被替換：{out}");
    assert_eq!(out.matches("tokengateway:").count(), 1, "不得重複建塊：{out}");
    // 既有塊缺 key_env：補上憑證指針（hermes 讀 .env 的依據）
    assert!(
        out.contains("    key_env: HERMES_CUSTOM_TOKENGATEWAY_API_KEY"),
        "{out}"
    );
    assert!(out.contains("    model: b"), "塊內 model 同步更新：{out}");
}


#[test]
fn hermes_key_env_inserted_once_and_idempotent() {
    let src = "providers:\n  tokengateway:\n    name: tokengateway\n    base_url: http://127.0.0.1:15722/v1\n    model: old\n    discover_models: false\n";
    let once = ensure_hermes_provider(src, "http://127.0.0.1:15722/v1", "deepseek-v4.1-flash");
    assert_eq!(once.matches("key_env:").count(), 1, "{once}");
    assert!(once.contains("    key_env: HERMES_CUSTOM_TOKENGATEWAY_API_KEY"), "{once}");
    assert!(once.contains("    extra_headers:\n      X-TG-App: hermes"), "{once}");
    let twice = ensure_hermes_provider(&once, "http://127.0.0.1:15722/v1", "deepseek-v4.1-flash");
    assert_eq!(twice.matches("key_env:").count(), 1, "重複套用不得再插入：{twice}");
    assert_eq!(twice.matches("extra_headers:").count(), 1, "{twice}");
    assert_eq!(twice.matches("X-TG-App:").count(), 1, "{twice}");
    assert_eq!(twice.matches("    model:").count(), 1, "{twice}");
}


#[test]
fn hermes_parse_and_base_lookup() {
    let text = "# c\nmodel:\n  provider: flatkey\n  default: deepseek-v4-flash\nproviders:\n  flatkey:\n    name: flatkey\n    base_url: https://router.flatkey.ai/v1\n";
    let (p, d) = parse_hermes_model(text);
    assert_eq!(p.as_deref(), Some("flatkey"));
    assert_eq!(d.as_deref(), Some("deepseek-v4-flash"));
    assert_eq!(
        hermes_provider_base(text, "flatkey").as_deref(),
        Some("https://router.flatkey.ai/v1")
    );
    assert_eq!(hermes_provider_base(text, "nope"), None);
}


#[test]
fn dotenv_replace_and_append() {
    let old = "# c\nA=1\nHERMES_CUSTOM_TOKENGATEWAY_API_KEY=old\n";
    let out = dotenv_set(old, "HERMES_CUSTOM_TOKENGATEWAY_API_KEY", "new");
    assert!(out.contains("HERMES_CUSTOM_TOKENGATEWAY_API_KEY=new"), "{out}");
    assert!(!out.contains("=old"), "{out}");
    assert!(out.contains("# c"), "註釋必須保留");
    let out2 = dotenv_set("A=1\n", "B", "2");
    assert!(out2.ends_with("B=2\n"), "{out2}");
}


#[test]
fn plan_rejects_unknown_app_and_empty_model() {
    let req = SwitchRequest {
        app: "nope".into(),
        base_url: "http://x".into(),
        api_key: "k".into(),
        model: "m".into(),
        via_gateway: true,
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
