//! 各工具接管狀態偵測
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub app: String,
    pub display: String,
    /// 執行檔在 PATH 或存在設定檔證據。
    pub installed: bool,
    pub config_path: Option<String>,
    pub current_base_url: Option<String>,
    pub current_model: Option<String>,
    /// 當前 base_url 是否指向本網關連接埠。
    pub gateway_active: bool,
    /// 是否支援自動切換寫入（dsh 暫為 false：僅檢測 + 金鑰環境變量指引）。
    pub supported_switch: bool,
}


pub(crate) fn detect_claude(port: u16) -> ToolStatus {
    let mut st = ToolStatus {
        app: "claude".into(),
        display: "Claude Code".into(),
        installed: on_path(&["claude"]),
        config_path: None,
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: true,
    };
    let Ok(home) = user_home() else { return st };
    let p = home.join(".claude").join("settings.json");
    if p.exists() {
        st.installed = true;
        st.config_path = Some(p.to_string_lossy().to_string());
        if let Some(text) = read_text(&p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                let env = &v["env"];
                st.current_base_url = env["ANTHROPIC_BASE_URL"].as_str().map(|s| s.to_string());
                st.current_model = env["ANTHROPIC_DEFAULT_SONNET_MODEL"]
                    .as_str()
                    .or(env["ANTHROPIC_DEFAULT_OPUS_MODEL"].as_str())
                    .map(|s| s.to_string());
            }
        }
    }
    st.gateway_active = st
        .current_base_url
        .as_deref()
        .is_some_and(|u| is_gateway(u, port));
    st
}


pub(crate) fn detect_codex(port: u16) -> ToolStatus {
    let mut st = ToolStatus {
        app: "codex".into(),
        display: "Codex".into(),
        installed: on_path(&["codex"]),
        config_path: None,
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: true,
    };
    let Ok(home) = user_home() else { return st };
    let p = home.join(".codex").join("config.toml");
    if p.exists() {
        st.installed = true;
        st.config_path = Some(p.to_string_lossy().to_string());
        if let Some(text) = read_text(&p) {
            // 注意：一律用 get（Index 讀缺失鍵會 panic；用戶配置可能無頂層 model，
            // 如 model 寫在某 provider 表下時）
            if let Ok(doc) = text.parse::<toml_edit::DocumentMut>() {
                st.current_model = doc
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let pid = doc
                    .get("model_provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("openai");
                st.current_base_url = doc
                    .get("model_providers")
                    .and_then(|t| t.get(pid))
                    .and_then(|t| t.get("base_url"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }
    st.gateway_active = st
        .current_base_url
        .as_deref()
        .is_some_and(|u| is_gateway(u, port));
    st
}


pub(crate) fn detect_opencode(port: u16) -> ToolStatus {
    let mut st = ToolStatus {
        app: "opencode".into(),
        display: "OpenCode".into(),
        installed: on_path(&["opencode"]),
        config_path: None,
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: true,
    };
    let Ok(home) = user_home() else { return st };
    let p = home.join(".config").join("opencode").join("opencode.json");
    if p.exists() {
        st.installed = true;
        st.config_path = Some(p.to_string_lossy().to_string());
        if let Some(text) = read_text(&p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                st.current_base_url = v["provider"][GATEWAY_PROVIDER_ID]["options"]["baseURL"]
                    .as_str()
                    .map(|s| s.to_string());
                st.current_model = v["model"].as_str().map(|s| s.to_string());
            }
        }
    }
    st.gateway_active = st
        .current_base_url
        .as_deref()
        .is_some_and(|u| is_gateway(u, port));
    st
}


pub(crate) fn detect_dsh() -> ToolStatus {
    let mut st = ToolStatus {
        app: "dsh".into(),
        display: "DeepSeek Harness".into(),
        installed: on_path(&["dsh"]),
        config_path: None,
        current_base_url: None,
        // endpoint 由 profile bundle 決定，M2 不解析；金鑰走 DEEPSEEK_API_KEY。
        current_model: Some("deepseek-v4-flash".to_string()),
        gateway_active: false,
        supported_switch: false,
    };
    if let Ok(home) = user_home() {
        if home.join(".dsh").exists() {
            st.installed = true;
        }
    }
    st
}


/// Cursor 與 Antigravity 已不再檢測（2026-09-28，第一階段 A）；
/// Hermes 已不再檢測（2026-09-28，第一階段 F）。
///
/// 理由（實查）：
/// - cursor／antigravity 本來就**只能檢測、不能接管**（`supported_switch: false`），
///   而且都沒安裝、0 筆請求、0 份備份。
/// - hermes 只有 1 筆請求，卻為了它養了 367 行專屬 YAML 邏輯。
///
/// ⚠️ `APPS`／`APP_META`／`APP_COLORS`／`infer_app` 裡的 cursor／antigravity／
/// hermes **刻意保留**：那四處是**用量歸屬與顯示**用的，不是工具偵測用的。刪掉的話，
/// 歷史資料若有這些請求會變成 `unknown`，用量篩選下拉也會少選項
/// —— 這正是「精簡不可以犧牲資訊」的落點。
pub fn detect_tools(port: u16) -> Vec<ToolStatus> {
    vec![
        detect_claude(port),
        detect_codex(port),
        detect_opencode(port),
        detect_dsh(),
    ]
}

// ---------------------------------------------------------------- 工具版本與更新 ---
