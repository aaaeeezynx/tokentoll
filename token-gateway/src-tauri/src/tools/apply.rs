//! 各工具設定寫入（claude／opencode 共用片段）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

pub fn config_path_for(app: &str) -> Result<PathBuf, String> {
    match app {
        "claude" => Ok(user_home()?.join(".claude").join("settings.json")),
        "codex" => Ok(user_home()?.join(".codex").join("config.toml")),
        "opencode" => Ok(user_home()?
            .join(".config")
            .join("opencode")
            .join("opencode.json")),
        "hermes" => Err("Hermes 已於第一階段（F）移除支援：只保留用量歸屬標籤，不再寫入它的設定".into()),
        "dsh" => Err("DSH 暫不支援自動寫入：endpoint 由 profile bundle 決定，M2 僅檢測 + 金鑰環境變量指引".into()),
        _ => Err(format!("未知工具：{app}")),
    }
}

// ---------------------------------------------------------------- 改寫 ---


/// Claude settings.json：合併 env，保留其他所有鍵。
/// map 非空檔位寫入 ANTHROPIC_MODEL / ANTHROPIC_DEFAULT_*（cc-switch 同款映射）。
///
/// `context_window` 已知時另寫 `CLAUDE_CODE_MAX_CONTEXT_TOKENS`：Claude Code 對
/// 它不認識的模型（我們寫進去的來源模型全都是）只**假設 200k**，於是 1M 級模型會提早
/// 自動壓縮 —— 這是它自己在 stderr 講的（`[claude-code:unrecognized_model]`）。
/// 不知道就**不寫**：寧可留給使用者，也不要編一個數字出來。
pub fn claude_apply(
    existing: Option<&str>,
    base_url: &str,
    token: &str,
    map: Option<&ClaudeModelMap>,
    context_window: Option<i64>,
) -> Result<String, String> {
    let mut v: serde_json::Value = match existing {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("settings.json 解析失敗：{e}"))?,
        None => serde_json::json!({}),
    };
    if !v.is_object() {
        v = serde_json::json!({});
    }
    let env = v
        .as_object_mut()
        .expect("object")
        .entry("env")
        .or_insert(serde_json::json!({}));
    if !env.is_object() {
        *env = serde_json::json!({});
    }
    let m = env.as_object_mut().expect("object");
    m.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        serde_json::Value::String(base_url.to_string()),
    );
    m.insert(
        "ANTHROPIC_AUTH_TOKEN".to_string(),
        serde_json::Value::String(token.to_string()),
    );
    if let Some(mm) = map {
        for (k, v) in mm.pairs() {
            let v = v.trim();
            if !v.is_empty() {
                m.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
    }
    if let Some(cw) = context_window.filter(|n| *n > 0) {
        m.insert(
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS".to_string(),
            serde_json::Value::String(cw.to_string()),
        );
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}


/// OpenCode opencode.json：合併 provider 段，apiKey 默寫 `{env:}` 引用；
/// 直连上游模式（direct_key=Some）時寫上游 Key 明文（與 Codex 的 experimental_bearer_token 對等）。
/// context 為模型上下文上限時寫入 `models.<id>.limit.context`。
pub fn opencode_apply(
    existing: Option<&str>,
    base_url: &str,
    model: &str,
    context_window: Option<i64>,
    direct_key: Option<&str>,
) -> Result<String, String> {
    let mut v: serde_json::Value = match existing {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("opencode.json 解析失敗：{e}"))?,
        None => serde_json::json!({}),
    };
    if !v.is_object() {
        v = serde_json::json!({});
    }
    let root = v.as_object_mut().expect("object");
    let provider = root
        .entry("provider")
        .or_insert(serde_json::json!({}));
    if !provider.is_object() {
        *provider = serde_json::json!({});
    }
    let pmap = provider.as_object_mut().expect("object");
    let entry = pmap
        .entry(GATEWAY_PROVIDER_ID)
        .or_insert(serde_json::json!({}));
    if !entry.is_object() {
        *entry = serde_json::json!({});
    }
    let emap = entry.as_object_mut().expect("object");
    emap.insert(
        "npm".to_string(),
        serde_json::Value::String("@ai-sdk/openai-compatible".to_string()),
    );
    emap.insert(
        "name".to_string(),
        serde_json::Value::String("Token Toll".to_string()),
    );
    let options = emap.entry("options").or_insert(serde_json::json!({}));
    if !options.is_object() {
        *options = serde_json::json!({});
    }
    let omap = options.as_object_mut().expect("object");
    omap.insert(
        "baseURL".to_string(),
        serde_json::Value::String(base_url.to_string()),
    );
    omap.insert(
        "apiKey".to_string(),
        serde_json::Value::String(match direct_key {
            Some(k) if !k.trim().is_empty() => k.trim().to_string(),
            _ => format!("{{env:{GATEWAY_ENV_KEY}}}"),
        }),
    );
    let models = emap.entry("models").or_insert(serde_json::json!({}));
    if !models.is_object() {
        *models = serde_json::json!({});
    }
    let mentry = models
        .as_object_mut()
        .expect("object")
        .entry(model)
        .or_insert(serde_json::json!({ "name": model }));
    if !mentry.is_object() {
        *mentry = serde_json::json!({ "name": model });
    }
    if let Some(cw) = context_window {
        if cw > 0 {
            mentry
                .as_object_mut()
                .expect("object")
                .entry("limit")
                .or_insert(serde_json::json!({}));
            let limit = &mut mentry["limit"];
            if !limit.is_object() {
                *limit = serde_json::json!({});
            }
            limit
                .as_object_mut()
                .expect("object")
                .insert("context".to_string(), serde_json::json!(cw));
        }
    }
    // 接管時一併指定模型。
    //
    // 只有 `provider.tokengateway.models.<模型>` 是不夠的：`opencode run`
    // 在**沒有**頂層 `model` 時會用它自己的預設模型（這台機器實測是
    // `claude-sonnet-4-6`），於是請求根本不會落到網關 —— 使用者看到的是
    // 「Anthropic API key is missing」這種與網關無關的錯誤。頂層 `model`
    // 的格式是 `<provider>/<model>`（OpenCode 自己的寫法）。
    //
    // 還原時 `opencode_native` 會把指向 `tokengateway/` 的這一鍵移除；
    // 使用者原本的 `model` 值由基準備份還原（見 `backup.rs`）。
    if !model.trim().is_empty() {
        root.insert(
            "model".to_string(),
            serde_json::Value::String(format!("{GATEWAY_PROVIDER_ID}/{model}")),
        );
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}


/// Claude Code 模型映射（空字串 = 不寫入該檔位，保留 settings.json 現值）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ClaudeModelMap {
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub haiku: String,
    #[serde(default)]
    pub sonnet: String,
    #[serde(default)]
    pub opus: String,
    #[serde(default)]
    pub fable: String,
}


impl ClaudeModelMap {
    pub(crate) fn pairs(&self) -> [(&'static str, &str); 5] {
        [
            ("ANTHROPIC_MODEL", self.default.as_str()),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL", self.haiku.as_str()),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", self.sonnet.as_str()),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", self.opus.as_str()),
            ("ANTHROPIC_DEFAULT_FABLE_MODEL", self.fable.as_str()),
        ]
    }
}


/// 工具原生格式（計劃頁相容提示用）。
pub(crate) fn tool_format(app: &str) -> &'static str {
    match app {
        "claude" => "anthropic",
        "codex" => "openai-responses",
        _ => "openai-chat",
    }
}


pub(crate) fn ps_export(var: &str) -> String {
    format!("$env:{var} = \"<把密鑰貼在這裡>\"  # 當前終端生效；長期使用請設為用戶環境變量")
}
