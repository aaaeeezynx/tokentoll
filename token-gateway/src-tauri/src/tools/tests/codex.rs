//! Codex 接管（含 B3／B4 迴歸）
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;


#[test]
fn gateway_request_overrides_conflicting_external_url() {
    let req = SwitchRequest {
        app: "codex".into(),
        base_url: "https://integrate.api.nvidia.com/v1".into(),
        api_key: "k".into(),
        model: "m".into(),
        via_gateway: true,
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
    assert!(out.contains("[model_providers.tokengateway]"), "別名段必須創建：{out}");
    // 共享段名方案：model_provider = custom（cc-switch 同款，舊會話可繼續）
    assert!(out.contains("model_provider = \"custom\""), "{out}");
    // custom 段內容由接管覆寫為網關（舊的 15721 直連內容被替換）
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    assert_eq!(
        v["model_providers"]["custom"]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    assert_eq!(
        v["model_providers"]["tokengateway"]["base_url"].as_str(),
        Some("http://127.0.0.1:15722/v1")
    );
    assert!(out.contains("env_key = \"TOKEN_GATEWAY_KEY\""), "{out}");
    assert!(out.contains("wire_api = \"responses\""), "{out}");
    assert!(out.contains("model = \"gpt-5.5\""), "{out}");
    assert!(out.contains("[mcp_servers]"), "無關段必須保留");
}


/// B4 迴歸測試：`wire_api` 不可無差別寫 `responses`。
///
/// **直連第三方**時 `base_url` 是對方（`Providers.tsx`：
/// `base_url: via ? gatewayUrl(...) : provider.base_url`），而多數第三方
/// 只實作 Chat Completions；硬寫 `responses` 會讓 Codex 打到不存在的
/// 端點而 404。原本只**警告**、沒有修。
#[test]
fn codex_wire_api_matches_upstream_capability() {
    // 指向本網關 → responses（網關兩種都收，且 responses 保留 reasoning）
    assert_eq!(codex_wire_api("http://127.0.0.1:15722/v1"), "responses");
    // 官方 OpenAI → responses
    assert_eq!(codex_wire_api("https://api.openai.com/v1"), "responses");
    // 第三方 → chat（唯一普遍實作的形狀）
    assert_eq!(codex_wire_api("https://integrate.api.nvidia.com/v1"), "chat");
    assert_eq!(codex_wire_api("https://api.deepseek.com/v1"), "chat");
    assert_eq!(codex_wire_api("https://openrouter.ai/api/v1"), "chat");
    // 大小寫與前後空白不影響判定
    assert_eq!(codex_wire_api("  HTTPS://API.OPENAI.COM/v1  "), "responses");
}


/// §5.3 第 2 層：provider 的**明確宣告**（`api_format`）優先於 URL 推定。
///
/// 這是「能力宣告」勝過「事後猜測」的落點。URL 推定只是後備：第三方也可
/// 能提供 responses 端點，光看網域猜不出來。
#[test]
fn codex_wire_api_prefers_declared_format() {
    // 宣告了就照宣告走
    assert_eq!(codex_wire_api_declared(Some("openai-chat")), Some("chat"));
    assert_eq!(
        codex_wire_api_declared(Some("openai-responses")),
        Some("responses")
    );
    assert_eq!(codex_wire_api_declared(Some("mixed")), Some("responses"));
    // 前後空白不影響
    assert_eq!(codex_wire_api_declared(Some("  openai-chat  ")), Some("chat"));
    // 宣告不足以判定 → None，交由 URL 推定
    assert_eq!(codex_wire_api_declared(Some("anthropic")), None);
    assert_eq!(codex_wire_api_declared(Some("gemini")), None);
    assert_eq!(codex_wire_api_declared(None), None);
    assert_eq!(codex_wire_api_declared(Some("")), None);

    // 關鍵對照：同一個第三方 URL，宣告能推翻 URL 推定。
    // 從 URL 猜會得到 chat（第三方通常只說 chat）……
    assert_eq!(codex_wire_api("https://api.example.com/v1"), "chat");
    // ……但若使用者宣告該渠道支援 responses，就該照宣告用 responses。
    let out = codex_apply(
        "model = \"m\"\n",
        "https://api.example.com/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth {
            direct_key: Some("k"),
            wire_api: codex_wire_api_declared(Some("openai-responses")),
        },
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    for (name, sec) in v["model_providers"].as_table().unwrap().iter() {
        assert_eq!(
            sec.as_table().and_then(|t| t.get("wire_api")).and_then(|x| x.as_str()),
            Some("responses"),
            "宣告為 openai-responses 時 [{name}] 應用 responses：{out}"
        );
    }
}


/// 沒有宣告時，`codex_apply` 必須回退到 URL 推定（不可變成無值）。
#[test]
fn codex_wire_api_falls_back_to_url_when_undeclared() {
    let out = codex_apply(
        "model = \"m\"\n",
        "https://api.example.com/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth {
            direct_key: None,
            wire_api: codex_wire_api_declared(Some("anthropic")),
        },
    )
    .unwrap();
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    for (name, sec) in v["model_providers"].as_table().unwrap().iter() {
        assert_eq!(
            sec.as_table().and_then(|t| t.get("wire_api")).and_then(|x| x.as_str()),
            Some("chat"),
            "未宣告時應回退為 URL 推定（chat）：[{name}] {out}"
        );
    }
}


/// B4 端到端：直連第三方時 `codex_apply` 產出的**每一個** provider 段
/// 都必須是 `wire_api = "chat"`；走網關時則維持 `responses`。
#[test]
fn codex_apply_picks_wire_api_per_upstream() {
    let get = |out: &str| -> Vec<(String, String)> {
        let v: toml_edit::DocumentMut = out.parse().unwrap();
        v["model_providers"]
            .as_table()
            .unwrap()
            .iter()
            .map(|(k, sec)| {
                let w = sec
                    .as_table()
                    .and_then(|t| t.get("wire_api"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("<缺失>")
                    .to_string();
                (k.to_string(), w)
            })
            .collect()
    };

    // 直連第三方 → 全部 chat
    let out = codex_apply(
        "model = \"m\"\n",
        "https://integrate.api.nvidia.com/v1",
        "m",
        None,
        None,
        &["oldalias".to_string()],
        &[],
        CodexAuth {
            direct_key: Some("nv-key"),
            ..Default::default()
        },
    )
    .unwrap();
    let got = get(&out);
    assert!(!got.is_empty(), "應產出 provider 段：{out}");
    for (name, w) in &got {
        assert_eq!(w, "chat", "第三方直連時 [{name}] 應為 chat：{out}");
    }
    assert!(
        got.iter().any(|(n, _)| n == "oldalias"),
        "別名段也必須存在：{got:?}"
    );

    // 走網關 → 全部 responses
    let out = codex_apply(
        "model = \"m\"\n",
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    for (name, w) in get(&out) {
        assert_eq!(w, "responses", "走網關時 [{name}] 應為 responses：{out}");
    }
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
            ..Default::default()
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
    assert!(out2.contains("env_key = \"TOKEN_GATEWAY_KEY\""), "{out2}");
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


#[test]
fn dsh_plan_is_detect_only() {        let req = SwitchRequest {
        app: "dsh".into(),
        base_url: "".into(),
        api_key: "k".into(),
        model: "deepseek-v4-flash".into(),
        via_gateway: false,
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
