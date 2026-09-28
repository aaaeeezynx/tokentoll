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


const NIM_CONFIG: &str = r#"model = "moonshotai/kimi-k3"
model_provider = "custom"
model_reasoning_effort = "medium"
disable_response_storage = true
model_catalog_json = 'C:\Users\x\AppData\Roaming\com.tokencounter.gateway\catalogs\codex-15.json'

[model_providers.custom]
name = "custom"
base_url = "https://integrate.api.nvidia.com/v1"
wire_api = "responses"

[model_providers.nvidia-nim]
name = "nvidia-nim"
base_url = "https://integrate.api.nvidia.com/v1"
wire_api = "responses"

[plugins."foo@bar"]
enabled = true
"#;


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
fn claude_native_keeps_users_own_endpoint() {
    // 使用者自己的 router ＋ Claude 模型名稱：不是接管產物，一個字都不該改。
    let text = r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "",
    "ANTHROPIC_BASE_URL": "https://router.flatkey.ai/v1",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-5"
  },
  "theme": "dark"
}"#;
    let out = to_native("claude", text, 15722);
    assert!(out.changes.is_empty(), "{:?}", out.changes);
    assert!(out.warning.is_some(), "應說明沒有網關痕跡");
    let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
    assert_eq!(
        v["env"]["ANTHROPIC_BASE_URL"].as_str(),
        Some("https://router.flatkey.ai/v1")
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
fn unsupported_app_is_reported_not_silently_ignored() {
    let out = to_native("cursor", "whatever", 15722);
    assert_eq!(out.text, "whatever");
    assert!(out.warning.unwrap().contains("cursor"));
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
