//! Codex 接管（含 B3 迴歸）
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。
//!
//! B4／§5.3 的 `wire_api` 判定測試已於 2026-09-28 再拆到
//! `codex_wire_api.rs`（本檔當時 566 行 > 400）。

use super::*;


#[test]
fn gateway_request_overrides_conflicting_external_url() {
    let req = SwitchRequest {
        app: "codex".into(),
        base_url: "https://integrate.api.nvidia.com/v1".into(),
        api_key: "k".into(),
        model: "m".into(),
        provider_id: Some(15),
        provider_format: Some("openai-chat".into()),
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: true,
        key_id: None,
        claude_map: None,
    };
    let plan = plan_switch(&req, 15722).unwrap();
    assert!(plan.edits.iter().any(|e| e.contains("http://127.0.0.1:15722/v1")), "{plan:?}");
    assert!(!plan.edits.iter().any(|e| e.contains("integrate.api.nvidia.com")), "{plan:?}");
}


#[test]
fn codex_preserves_comments_and_sections() {
    let old = "# cc-switch managed\nmodel = \"old\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15721/v1\"\n\n[mcp_servers]\n";
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "gpt-5.5",
        None,
        None,
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    assert!(out.contains("# cc-switch managed"), "註釋必須保留：{out}");
    assert!(out.contains("[model_providers.custom]"), "共享段必須存在");
    assert!(
        out.contains(&format!("[model_providers.{GATEWAY_PROVIDER_ID}]")),
        "渠道段必須創建：{out}"
    );
    // 共享段名方案：model_provider = custom（cc-switch 同款，舊會話可繼續）
    assert!(out.contains("model_provider = \"custom\""), "{out}");
    // custom 段內容由接管覆寫為網關（舊的 15721 直連內容被替換）
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    assert_eq!(
        v["model_providers"]["custom"]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    assert_eq!(
        v["model_providers"][GATEWAY_PROVIDER_ID]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    // 改名相容：新舊段名都要建立（詳見 tests/rebrand.rs）
    assert!(out.contains("env_key = \"TOKEN_TOLL_KEY\""), "{out}");
    assert!(out.contains("wire_api = \"responses\""), "{out}");
    assert!(out.contains("model = \"gpt-5.5\""), "{out}");
    assert!(out.contains("[mcp_servers]"), "無關段必須保留");
}


#[test]
fn codex_shared_brand_keeps_legacy_sections() {
    // 模擬用戶實際 config：custom 段已被外部刪除，僅剩 tokengateway
    let old = "model = \"deepseek-ai/deepseek-v4-flash-0731\"\nmodel_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nname = \"Token Gateway\"\nbase_url = \"http://127.0.0.1:15722/v1\"\nenv_key = \"TOKEN_GATEWAY_KEY\"\nwire_api = \"responses\"\n";
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    // 兩段都指向網關：custom 時代與 tokengateway 時代的舊會話都能解析
    assert_eq!(
        v["model_providers"]["custom"]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    assert_eq!(
        v["model_providers"]["tokengateway"]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    assert_eq!(v["model_provider"].as_str(), Some("custom"));
}


#[test]
fn codex_reasoning_and_catalog_fields() {
    let old = "model = \"a\"\n";
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        Some("high"),
        Some("C:\\data\\catalogs\\codex-1.json"),
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    assert!(out.contains("model_reasoning_effort = \"high\""), "{out}");
    // toml_edit 對含反斜線路徑用單引號字面量輸出，只斷言關鍵片段
    assert!(out.contains("model_catalog_json"), "{out}");
    assert!(out.contains("codex-1.json"), "{out}");
    // unset / 空不寫入
    let out2 = codex_apply(old, "http://x", "m", Some("unset"), None, &[], &[], CodexAuth::default()).unwrap();
    assert!(!out2.contains("model_reasoning_effort"), "{out2}");
    assert!(!out2.contains("model_catalog_json"), "{out2}");
}


#[test]
fn codex_apply_twice_is_idempotent() {
    // 連續接管兩次：託管段恰好一次，註釋與無關段保留（P0 重複段回歸）
    let old = "# keep\nmodel = \"a\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15721/v1\"\n\n[mcp_servers]\n";
    let once = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &["nim-direct".to_string()],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    let twice = codex_apply(
        &once,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &["nim-direct".to_string()],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    for id in ["custom", "tokengateway", "nim-direct"] {
        let header = format!("[model_providers.{id}]");
        assert_eq!(
            twice.lines().filter(|l| l.trim() == header).count(),
            1,
            "段 {header} 必須恰好一次：{twice}"
        );
    }
    assert!(twice.contains("# keep"), "{twice}");
    assert!(twice.contains("[mcp_servers]"), "{twice}");
    // 冪等：第二次輸出與第一次一致
    assert_eq!(once, twice);
}


#[test]
fn codex_apply_sanitizes_legacy_duplicates() {
    // P0 真實損壞：tokengateway 段重複 → 非法 TOML，接管入口直接消毒
    let old = "model = \"a\"\n\n[model_providers.tokengateway]\nbase_url = \"http://x/v1\"\n\n[model_providers.tokengateway]\nbase_url = \"http://x/v1\"\n";
    assert!(old.parse::<toml_edit::DocumentMut>().is_err(), "夾具必須是非法 TOML");
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    assert_eq!(
        out.lines()
            .filter(|l| l.trim() == "[model_providers.tokengateway]")
            .count(),
        1,
        "{out}"
    );
    out.parse::<toml_edit::DocumentMut>().unwrap();
}


#[test]
fn codex_aliases_and_inline_models() {
    let old = "model = \"a\"\n";
    let aliases = vec![
        "nim-direct".to_string(),
        "opencode-zen".to_string(),
        "custom".to_string(),       // 與共享段重名應去重
        "bad key!".to_string(),     // 非法字元應丟棄
    ];
    let inline = vec![("m1".to_string(), "模型一".to_string())];
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &aliases,
        &inline,
        CodexAuth::default(),
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    for id in ["custom", "tokengateway", "nim-direct", "opencode-zen"] {
        assert_eq!(
            v["model_providers"][id]["base_url"].as_str(),
            Some("http://127.0.0.1:15722/v1"),
            "別名段 {id} 必須指向網關"
        );
        // 內聯 models（桌面版選擇器顯示用）
        let models = v["model_providers"][id]["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        let got = match models.iter().next().unwrap() {
            toml_edit::Value::InlineTable(t) => {
                t.get("model").and_then(|m| m.as_str()).map(|s| s.to_string())
            }
            _ => None,
        };
        assert_eq!(got.as_deref(), Some("m1"));
    }
    assert!(v.get("model_providers").and_then(|m| m.get("bad key!")).is_none());
}


/// 真實事故（2026-09-29）：設定檔裡已經被寫進 `[model_providers.openai]`
/// （Codex 的內建 id，不可定義）→ 整份 config.toml 載入失敗，使用者看到
/// 「無法登入」。接管必須把它清掉，否則錯誤永遠不會好。
#[test]
fn codex_apply_removes_reserved_builtin_provider_sections() {
    let old = "model = \"a\"\n\
               [model_providers.openai]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
               [model_providers.ollama]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
               [model_providers.lmstudio]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
               [model_providers.keepme]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &["keepme".to_string()],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    let mp = v.get("model_providers").and_then(|m| m.as_table()).unwrap();
    for bad in CODEX_RESERVED_PROVIDER_IDS {
        assert!(mp.get(bad).is_none(), "內建段 `{bad}` 必須被清掉：{out}");
    }
    assert!(mp.get("keepme").is_some(), "一般別名段不受影響：{out}");
    assert!(is_reserved_codex_provider("OpenAI"));
    assert!(is_reserved_codex_provider(" openai "));
    assert!(!is_reserved_codex_provider("openai-custom"));
    assert!(!is_reserved_codex_provider("custom"));
}


#[test]
fn codex_direct_writes_bearer_no_envkey() {
    let old = "model = \"a\"\n";
    let out = codex_apply(
        old,
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth {
            direct_key: Some("nv-direct-secret"),
        },
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    for id in ["custom", "tokengateway"] {
        assert_eq!(
            v["model_providers"][id]["experimental_bearer_token"].as_str(),
            Some("nv-direct-secret"),
            "{out}"
        );
        assert!(
            v["model_providers"][id].get("env_key").is_none(),
            "直連模式不寫 env_key：{out}"
        );
    }
    // 非直連保持 env_key
    let out2 = codex_apply(old, "http://x", "m", None, None, &[], &[], CodexAuth::default()).unwrap();
    assert!(out2.contains("env_key = \"TOKEN_TOLL_KEY\""), "{out2}");
}


#[test]
fn codex_doctor_runs_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "model_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
    )
    .unwrap();
    let before = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let report = codex_doctor_at(dir.path(), 15722);
    assert!(!report.is_empty());
    assert!(report.iter().any(|l| l.contains("15721")), "{report:?}");
    // 純讀取：檔案未被改動
    assert_eq!(std::fs::read_to_string(dir.path().join("config.toml")).unwrap(), before);
}


/// **回歸測試（2026-09-28）**：體檢的 `TOKEN_GATEWAY_KEY` 檢查原本
/// **無條件**報 ❌，於是「直連模式」（上游 Key 明文寫進 `config.toml`，
/// 根本不讀環境變數）的使用者每次體檢都看到假警報 —— 久而久之就不再看體檢。
///
/// 現在改成看設定檔**實際有沒有用到** `env_key`。
#[test]
fn doctor_env_key_warning_depends_on_config_usage() {
    // (1) 直連模式：provider 段寫 experimental_bearer_token，不需要環境變數。
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "model_provider = \"custom\"\n\n[model_providers.custom]\n\
         base_url = \"http://127.0.0.1:15722/v1\"\n\
         experimental_bearer_token = \"oc_sk_dummy\"\n\
         wire_api = \"responses\"\n",
    )
    .unwrap();
    let report = codex_doctor_at(dir.path(), 15722);
    assert!(
        report.iter().any(|l| l.contains("不需要")),
        "直連模式不該被要求設環境變數：{report:?}"
    );
    assert!(
        !report
            .iter()
            .any(|l| l.contains("❌") && (l.contains(GATEWAY_ENV_KEY) || l.contains(GATEWAY_ENV_KEY_LEGACY))),
        "直連模式不該對環境變數報 ❌（假警報）：{report:?}"
    );

    // (2) env_key 模式：這時環境變數才真的重要。
    //     測試行程通常沒設 TOKEN_GATEWAY_KEY；若真的設了，則應報 ✅。
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "model_provider = \"custom\"\n\n[model_providers.custom]\n\
         base_url = \"http://127.0.0.1:15722/v1\"\n\
         env_key = \"TOKEN_GATEWAY_KEY\"\n\
         wire_api = \"responses\"\n",
    )
    .unwrap();
    let report = codex_doctor_at(dir.path(), 15722);
    // 報告會寫出**實際命中的那個名字**（新名優先、舊名相容），所以兩個都算合格。
    let mentions = |l: &String| l.contains(GATEWAY_ENV_KEY) || l.contains(GATEWAY_ENV_KEY_LEGACY);
    assert!(
        report
            .iter()
            .any(|l| mentions(l) && (l.contains("❌") || l.contains("✅"))),
        "env_key 模式必須對環境變數給出明確結論：{report:?}"
    );
    assert!(
        !report.iter().any(|l| l.contains("不需要")),
        "設定檔用了 env_key 就不能說不需要：{report:?}"
    );
}


#[test]
fn dsh_plan_is_detect_only() {        let req = SwitchRequest {
        app: "dsh".into(),
        base_url: "".into(),
        api_key: "k".into(),
        model: "deepseek-v4-flash".into(),
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
    let plan = plan_switch(&req, 15721).unwrap();
    assert!(!plan.supported);
    assert!(plan.env_setup.iter().any(|s| s.contains("DEEPSEEK_API_KEY")));
}
