//! 「關閉網關 → 回到原生來源」的測試。
//!
//! 由 `tools.rs` 的測試模組拆分而來（與 `restore.rs` 同層）。共用 helper 由
//! 父模組 `tools::tests` 提供，經 `use super::*;` 取用。
//!
//! 這裡釘住的性質：
//! 1. 原生化**只動來源相關的鍵** —— 使用者自己的設定、以及舊對話要用的
//!    別名 provider 段一律保留。
//! 2. 原生化是**冪等**的（再套一次不會產生差異）。
//! 3. 設定檔壞掉時**不猜**：原樣返回並附警告，而不是寫出半套設定。
//! 4. 關閉流程 = 忠實還原 ＋ 原生化，兩件事都真的發生。

use super::*;


#[test]
fn codex_native_switches_to_openai_gpt_and_keeps_alias_sections() {
    let out = to_native("codex", NIM_CONFIG, 15722);
    assert!(out.warning.is_none(), "{:?}", out.warning);
    assert_eq!(out.changes.len(), 3, "{:?}", out.changes);

    let doc: toml_edit::DocumentMut = out.text.parse().expect("仍是合法 TOML");
    assert_eq!(doc["model_provider"].as_str(), Some("openai"));
    assert_eq!(doc["model"].as_str(), Some("gpt-5.6-luna"));
    assert!(doc.get("model_catalog_json").is_none(), "catalog 必須移除");

    // 使用者自己的鍵與舊對話要用的別名段：一個都不能少。
    assert_eq!(doc["model_reasoning_effort"].as_str(), Some("medium"));
    assert_eq!(doc["disable_response_storage"].as_bool(), Some(true));
    assert!(doc.get("model_providers").is_some(), "別名段必須保留");
    assert!(out.text.contains("[model_providers.nvidia-nim]"), "{}", out.text);
    assert!(out.text.contains("[plugins.\"foo@bar\"]"), "{}", out.text);
}


#[test]
fn codex_native_is_idempotent() {
    let once = to_native("codex", NIM_CONFIG, 15722);
    let twice = to_native("codex", &once.text, 15722);
    assert_eq!(twice.text, once.text);
    assert!(twice.changes.is_empty(), "{:?}", twice.changes);
}


#[test]
fn codex_native_removes_reserved_builtin_provider_sections() {
    // 真實事故（2026-09-29）：接管把 `openai` 當成別名段寫進設定，Codex 因而
    // 拒絕載入整份 config.toml（內建 id 不可覆寫），使用者看到「無法登入」。
    // 還原（回到官方）是最適合清掉它的時機。
    let dirty = "model_provider = \"custom\"\nmodel = \"m\"\n\
                 [model_providers.openai]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
                 [model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
    let out = to_native("codex", dirty, 15722);
    assert!(out.warning.is_none(), "{:?}", out.warning);
    let doc: toml_edit::DocumentMut = out.text.parse().expect("仍是合法 TOML");
    assert_eq!(doc["model_provider"].as_str(), Some("openai"));
    let mp = doc.get("model_providers").and_then(|m| m.as_table());
    assert!(
        mp.map(|m| m.get("openai").is_none()).unwrap_or(true),
        "內建段必須被清掉：{}",
        out.text
    );
    assert!(
        out.changes.iter().any(|c| c.contains("內建 provider 段")),
        "要告訴使用者做了什麼：{:?}",
        out.changes
    );
    // 冪等：再跑一次不該又冒出變更。
    let again = to_native("codex", &out.text, 15722);
    assert_eq!(again.text, out.text);
    assert!(again.changes.is_empty(), "{:?}", again.changes);
}


#[test]
fn codex_native_inserts_top_level_keys_before_tables() {
    // 沒有 model_provider / model 的畸形配置：新鍵必須留在根表格，
    // 不能被序列化到最後一個 [model_providers.*] 段裡面。
    let text = "[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\n";
    let out = to_native("codex", text, 15722);
    let doc: toml_edit::DocumentMut = out.text.parse().expect("合法 TOML");
    assert_eq!(doc["model_provider"].as_str(), Some("openai"));
    assert_eq!(doc["model"].as_str(), Some("gpt-5.6-luna"));
    // 根表格的鍵不會被吃進 [model_providers.custom]：上面兩個斷言若成立就代表
    // 它們在根層級（TOML 解析會把標頭之後的鍵歸給那個表）。
    let custom = doc["model_providers"]["custom"].as_table().expect("custom 表");
    assert!(!custom.contains_key("model_provider"), "{}", out.text);
}


#[test]
fn codex_native_leaves_invalid_toml_alone() {
    let out = to_native("codex", "model_provider = \"custom\"\n[oops\n", 15722);
    assert!(out.warning.is_some());
    assert!(out.changes.is_empty());
    assert_eq!(out.text, "model_provider = \"custom\"\n[oops\n");
}


const CLAUDE_MANAGED: &str = r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-local-abc",
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:15722",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4.1-flash",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "claude-opus-5",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-v4.1-flash",
    "ANTHROPIC_MODEL": "muse-spark-1.3-contributor"
  },
  "theme": "dark"
}"#;


#[test]
fn claude_native_drops_gateway_and_foreign_models_only() {
    let out = to_native("claude", CLAUDE_MANAGED, 15722);
    assert!(out.warning.is_none(), "{:?}", out.warning);
    let v: serde_json::Value = serde_json::from_str(&out.text).expect("仍是合法 JSON");
    let env = v["env"].as_object().expect("env 還在");

    assert!(!env.contains_key("ANTHROPIC_BASE_URL"), "網關端點必須移除");
    assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"), "網關權杖必須移除");
    assert!(!env.contains_key("ANTHROPIC_MODEL"));
    assert!(!env.contains_key("ANTHROPIC_DEFAULT_OPUS_MODEL"));
    // Claude 家族的鍵（含 _NAME）留著。
    assert_eq!(
        env["ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"].as_str(),
        Some("claude-opus-5")
    );
    assert_eq!(v["theme"].as_str(), Some("dark"));
    assert!(!env.contains_key("ANTHROPIC_DEFAULT_HAIKU_MODEL"), "非 claude-* 一律移除");
}


#[test]
fn claude_native_drops_gateway_context_override() {
    // 這個鍵是 `claude_apply` 為「網關上的來源模型」寫的；回到原生 Claude 就不能留，
    // 否則官方模型的視窗會被寫成來源模型的數字。
    let text = r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:15722",
  "CLAUDE_CODE_MAX_CONTEXT_TOKENS":"1000000","ANTHROPIC_DEFAULT_OPUS_MODEL_NAME":"claude-opus-5"}}"#;
    let out = to_native("claude", text, 15722);
    let v: serde_json::Value = serde_json::from_str(&out.text).expect("仍是合法 JSON");
    let env = v["env"].as_object().expect("env 還在");
    assert!(
        !env.contains_key("CLAUDE_CODE_MAX_CONTEXT_TOKENS"),
        "網關模型的視窗上限必須移除"
    );
    assert!(
        out.changes
            .iter()
            .any(|c| c.contains("CLAUDE_CODE_MAX_CONTEXT_TOKENS")),
        "要在訊息裡講清楚：{:?}",
        out.changes
    );
    // 別的鍵不受影響。
    assert_eq!(
        env["ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"].as_str(),
        Some("claude-opus-5")
    );
}


#[test]
fn claude_native_strips_users_own_third_party_endpoint() {
    // 2026-09-28 使用者選定「連端點也推回 Anthropic 官方」：他自己原本的 router
    // 也一樣移除（代價是要先登入一次）。模型名稱本來就是 Claude，留著。
    let text = r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "oc_sk_secret_value",
    "ANTHROPIC_BASE_URL": "https://opencode.ai/zen/go/v1",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-5"
  },
  "theme": "dark"
}"#;
    let out = to_native("claude", text, 15722);
    assert!(out.warning.is_none(), "{:?}", out.warning);
    let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
    let env = v["env"].as_object().unwrap();
    assert!(!env.contains_key("ANTHROPIC_BASE_URL"), "第三方 router 也要移除");
    assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"), "連帶的權杖一起移除");
    assert_eq!(
        env["ANTHROPIC_DEFAULT_SONNET_MODEL"].as_str(),
        Some("claude-sonnet-5")
    );
    // 訊息只講端點，不該把權杖值寫進去。
    assert!(out.changes.iter().any(|c| c.contains("opencode.ai/zen/go/v1")), "{:?}", out.changes);
    assert!(!out.text.contains("oc_sk_secret_value"), "權杖不該留在檔案裡");
}


#[test]
fn claude_native_keeps_official_endpoint_untouched() {
    // 已經是官方端點：沒有可原生化的地方，原樣返回並附說明。
    let text = r#"{
  "env": {
    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-5"
  }
}"#;
    let out = to_native("claude", text, 15722);
    assert!(out.changes.is_empty(), "{:?}", out.changes);
    assert!(out.warning.is_none(), "不是異常，不該示警：{:?}", out.warning);
    assert!(out.note.is_some(), "應說明沒有可原生化的地方");
    let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
    assert_eq!(
        v["env"]["ANTHROPIC_BASE_URL"].as_str(),
        Some("https://api.anthropic.com")
    );
}


const OPENCODE_MANAGED: &str = r#"{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "anthropic": {
      "options": {
        "apiKey": "sk-local-h6M09",
        "baseURL": "http://127.0.0.1:15722"
      }
    },
    "tokengateway": {
      "name": "Token Gateway",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "apiKey": "{env:TOKEN_GATEWAY_KEY}",
        "baseURL": "https://integrate.api.nvidia.com/v1"
      }
    }
  }
}"#;


#[test]
fn opencode_native_drops_gateway_endpoint_and_local_key() {
    let out = to_native("opencode", OPENCODE_MANAGED, 15722);
    assert!(out.warning.is_none(), "{:?}", out.warning);
    let v: serde_json::Value = serde_json::from_str(&out.text).expect("仍是合法 JSON");
    let opts = v["provider"]["anthropic"]["options"]
        .as_object()
        .expect("options 還在");
    assert!(!opts.contains_key("baseURL"), "網關端點必須移除");
    assert!(!opts.contains_key("apiKey"), "網關本地 key 必須移除");
    // 舊對話要用的 provider 段整段保留。
    assert_eq!(v["provider"]["tokengateway"]["name"].as_str(), Some("Token Gateway"));
    assert_eq!(
        v["provider"]["tokengateway"]["options"]["baseURL"].as_str(),
        Some("https://integrate.api.nvidia.com/v1")
    );
}


#[test]
fn opencode_native_drops_pinned_gateway_model_only() {
    // 接管時釘住的 `tokengateway/<模型>` 要移除，使用者自己的 model 不准動。
    let pinned = r#"{"provider": {}, "model": "tokengateway/deepseek-v4.1-flash"}"#;
    let out = to_native("opencode", pinned, 15722);
    let v: serde_json::Value = serde_json::from_str(&out.text).expect("仍是合法 JSON");
    assert!(v.get("model").is_none(), "釘住的網關模型必須移除：{}", out.text);
    assert!(
        out.changes.iter().any(|c| c.contains("model")),
        "要交代改了什麼：{:?}",
        out.changes
    );

    let own = r#"{"provider": {}, "model": "anthropic/claude-sonnet-4-6"}"#;
    let out2 = to_native("opencode", own, 15722);
    let v2: serde_json::Value = serde_json::from_str(&out2.text).expect("仍是合法 JSON");
    assert_eq!(
        v2["model"], "anthropic/claude-sonnet-4-6",
        "使用者自己的 model 不得移除"
    );
}


#[test]
fn opencode_native_reports_clean_config_as_note_not_warning() {
    // 2026-09-28 實機踩到：對一個本來就乾淨的 OpenCode 按關閉，畫面出現
    // 「⚠️ 設定裡沒有網關痕跡，維持原樣」—— 健康的設定不該被示警。
    let clean = r#"{"provider": {"anthropic": {"options": {}}, "tokengateway": {"options": {"baseURL": "https://integrate.api.nvidia.com/v1"}}}}"#;
    let out = to_native("opencode", clean, 15722);
    assert!(out.changes.is_empty(), "{:?}", out.changes);
    assert!(out.warning.is_none(), "不該示警：{:?}", out.warning);
    assert_eq!(out.note.as_deref(), Some("設定裡沒有網關痕跡，維持原樣"));
}


#[test]
fn unsupported_app_is_reported_not_silently_ignored() {
    let out = to_native("cursor", "whatever", 15722);
    assert_eq!(out.text, "whatever");
    assert!(out.warning.unwrap().contains("cursor"));
}


#[test]
fn switch_off_accepts_a_native_codex_backup() {
    // 2026-09-29 實測踩到：Codex 已經在原生來源時（`model_provider = "openai"`，
    // 而 Codex 的 `openai` provider 是內建的、沒有 `[model_providers.openai]` 段），
    // 關閉網關會失敗並顯示「所有備份都還含網關配置，無法還原到接管前」——
    // 明明那份備份一個網關位址都沒有。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    let native = r#"model = "gpt-5.6-luna"
model_provider = "openai"

[model_providers.custom]
name = "Token Gateway"
base_url = "https://integrate.api.nvidia.com/v1"
experimental_bearer_token = "nvapi-x"
wire_api = "responses"
"#;
    std::fs::write(bdir.join("config.toml.bak-20260929-025022"), native).unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "model_provider = \"custom\"\n").unwrap();

    let back = match restore_native_to_port(app_data, "codex", &cfg, 15722) {
        Ok(m) => m,
        Err(e) => panic!("原生備份應該可以被還原，卻失敗了：{e}"),
    };
    // migrate=true：乾淨備份會先被升成不可變基線，所以訊息裡是 baseline 而不是 bak。
    assert!(back.contains("config.toml.baseline-"), "{back}");
    assert!(back.contains("provider=openai"), "{back}");
    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(after.contains("model_provider = \"openai\""), "{after}");
    assert!(after.contains("[model_providers.custom]"), "別名段要留著：{after}");
}


#[test]
fn switch_off_accepts_a_native_claude_backup() {
    // 同一個失效的 Claude 版本：回到官方之後 `ANTHROPIC_BASE_URL` 不存在，
    // 於是「有沒有一個 URL」的判斷把乾淨備份判成沒有來源。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let bdir = app_data.join("backups").join("claude");
    std::fs::create_dir_all(&bdir).unwrap();
    let native = r#"{"env": {"ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-5"}, "theme": "dark"}"#;
    std::fs::write(bdir.join("settings.json.bak-20260929-030000"), native).unwrap();
    let cfg = dir.path().join("settings.json");
    std::fs::write(&cfg, r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:15722"}}"#).unwrap();

    let back = match restore_native_to_port(app_data, "claude", &cfg, 15722) {
        Ok(m) => m,
        Err(e) => panic!("原生備份應該可以被還原，卻失敗了：{e}"),
    };
    assert!(back.contains("settings.json.baseline-"), "{back}");
    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(!after.contains("15722"), "還原後不該再指向網關：{after}");
    assert!(after.contains("claude-sonnet-5"), "{after}");
}


#[test]
fn switch_off_restores_backup_then_converts_to_native() {
    // 接管前是 NIM 直連（乾淨備份），另有一份含網關痕跡的備份必須被跳過。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "model_provider = \"custom\"\n").unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(bdir.join("config.toml.bak-20260101-000000"), NIM_CONFIG).unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260201-000000"),
        "model_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
    )
    .unwrap();

    // migrate=true：乾淨備份會先被升成不可變基線（檔名時間戳最新），所以
    // 「還原自」指的是基線、且這條路徑不會有「跳過 N 個」註記。
    // （跳過含網關痕跡備份的行為由 `tests/restore.rs` 覆蓋。）
    let msg = restore_native_to_port(app_data, "codex", &cfg, 15722).unwrap();
    assert!(msg.contains("還原自 config.toml.baseline-"), "{msg}");
    assert!(msg.contains("已切回原生來源"), "{msg}");
    // 摘要重算過：不該再留著針對別名段、已經不成立的「直連第三方」警告。
    assert!(msg.contains("provider=openai model=gpt-5.6-luna"), "{msg}");
    assert!(!msg.contains("不可用請改走網關接管"), "{msg}");

    let doc: toml_edit::DocumentMut =
        std::fs::read_to_string(&cfg).unwrap().parse().expect("合法 TOML");
    assert_eq!(doc["model_provider"].as_str(), Some("openai"), "真的寫進檔案了");
    assert_eq!(doc["model"].as_str(), Some("gpt-5.6-luna"));
    assert!(doc.get("model_catalog_json").is_none());
    assert!(doc.get("model_providers").is_some());
}
