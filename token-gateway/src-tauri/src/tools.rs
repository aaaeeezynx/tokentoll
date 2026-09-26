//! M2：五工具渠道檢測與切換適配。
//!
//! 安全約定：
//! - 金鑰一律走環境變量 [`GATEWAY_ENV_KEY`]（Codex `env_key` / OpenCode `{env:}` /
//!   終端機 export），絕不寫入各工具設定檔；唯一的例外是 Claude settings.json 的
//!   `env`（靜態值，無引用語法，只能寫明文——網關模式下請使用可吊銷的本地 sk key）
//!   與 Hermes `.env`（其自有約定，復刻 flatkey 模式）。
//! - 每次寫入前自動備份到應用程式數據目錄 `backups/<app>/`，保留 [`BACKUP_KEEP`] 份。
//! - 所有檔案改寫都是純函數（輸入舊文本 → 輸出新文本），可單測；IO 只在
//!   [`apply_switch`] 一處發生。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const GATEWAY_HOST: &str = "127.0.0.1";
/// 各工具引用本地 Key 的統一環境變量名。
pub const GATEWAY_ENV_KEY: &str = "TOKEN_GATEWAY_KEY";
/// 我們在各工具配置裡創建的渠道 ID。
pub const GATEWAY_PROVIDER_ID: &str = "tokengateway";
/// Codex 共享段名（cc-switch 同款）：會話歷史按段名引用供應商，段名必須
/// 永遠存在且指向網關，舊會話才不會因段被刪而「Model provider not found」。
pub const CODEX_SHARED_PROVIDER_ID: &str = "custom";
/// Hermes 自定義渠道按其自有約定讀取的 Key 名：`HERMES_CUSTOM_<PROVIDER>_API_KEY`。
pub const HERMES_CUSTOM_ENV_KEY: &str = "HERMES_CUSTOM_TOKENGATEWAY_API_KEY";
pub const BACKUP_KEEP: usize = 10;

/// (app_id, 顯示名稱)
pub const APPS: [(&str, &str); 7] = [
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("opencode", "OpenCode"),
    ("hermes", "Hermes Agent"),
    ("dsh", "DeepSeek Harness"),
    ("cursor", "Cursor"),
    ("antigravity", "Antigravity"),
];

pub fn user_home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "無法解析用戶主目錄".to_string())
}

/// Hermes 家目錄：`$HERMES_HOME` → 有 config.yaml 的 `%LOCALAPPDATA%/hermes` → `~/.hermes`。
pub fn hermes_home() -> PathBuf {
    if let Ok(v) = std::env::var("HERMES_HOME") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        let p = local.join("hermes");
        if p.join("config.yaml").exists() {
            return p;
        }
    }
    user_home()
        .map(|h| h.join(".hermes"))
        .unwrap_or_else(|_| PathBuf::from(".hermes"))
}

/// 按工具拼網關 URL（M2 由前端計算後傳入；M3 代理側複用本函數做權威拼接）。
#[allow(dead_code)]
pub fn gateway_url(port: u16, app: &str) -> String {
    let base = format!("http://{GATEWAY_HOST}:{port}");
    match app {
        "claude" => base,
        _ => format!("{base}/v1"),
    }
}

fn on_path(names: &[&str]) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    // Windows 上可執行檔可能帶副檔名，額外探測常見位置。
    let _ = &mut dirs;
    dirs.iter().any(|d| {
        names.iter().any(|n| {
            let p = d.join(n);
            if p.is_file() {
                return true;
            }
            #[cfg(windows)]
            {
                for ext in ["exe", "cmd", "bat", "ps1"] {
                    if d.join(format!("{n}.{ext}")).is_file() {
                        return true;
                    }
                }
            }
            false
        })
    })
}

fn read_text(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

fn strip_quotes(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"'))
            || (t.starts_with('\'') && t.ends_with('\'')))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn codex_text_base_url(text: &str) -> Option<String> {
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let provider = doc
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");
    doc.get("model_providers")
        .and_then(|p| p.get(provider))
        .and_then(|p| p.get("base_url"))
        .and_then(|u| u.as_str())
        .map(|u| u.to_string())
}

// ---------------------------------------------------------------- 檢測 ---

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

fn is_gateway(url: &str, port: u16) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|u| {
        u.scheme() == "http"
            && u.host_str() == Some(GATEWAY_HOST)
            && u.port() == Some(port)
    })
}

fn detect_claude(port: u16) -> ToolStatus {
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

fn detect_codex(port: u16) -> ToolStatus {
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

fn detect_opencode(port: u16) -> ToolStatus {
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

/// 解析 hermes config.yaml 的 `model:` 段（provider / default）。
fn parse_hermes_model(text: &str) -> (Option<String>, Option<String>) {
    let mut in_model = false;
    let mut provider = None;
    let mut default = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_model = trimmed.trim_end_matches(':') == "model" && trimmed.ends_with(':');
            continue;
        }
        if in_model && indent == 2 {
            if let Some(v) = trimmed.strip_prefix("provider:") {
                provider = Some(strip_quotes(v));
            } else if let Some(v) = trimmed.strip_prefix("default:") {
                let v = strip_quotes(v);
                if !v.is_empty() {
                    default = Some(v);
                }
            }
        }
    }
    (provider, default)
}

/// 取 hermes `providers.<name>.base_url`。
fn hermes_provider_base(text: &str, name: &str) -> Option<String> {
    let mut in_providers = false;
    let mut in_block = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_providers = trimmed.trim_end_matches(':') == "providers" && trimmed.ends_with(':');
            in_block = false;
            continue;
        }
        if !in_providers {
            continue;
        }
        if indent == 2 && trimmed.ends_with(':') {
            in_block = trimmed.trim_end_matches(':') == name;
            continue;
        }
        if in_block && indent == 4 {
            if let Some(v) = trimmed.strip_prefix("base_url:") {
                return Some(strip_quotes(v));
            }
        }
    }
    None
}

fn detect_hermes(port: u16) -> ToolStatus {
    let home = hermes_home();
    let p = home.join("config.yaml");
    let mut st = ToolStatus {
        app: "hermes".into(),
        display: "Hermes Agent".into(),
        installed: on_path(&["hermes"]) || p.exists(),
        config_path: p.exists().then(|| p.to_string_lossy().to_string()),
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: true,
    };
    if let Some(text) = read_text(&p) {
        let (provider, default) = parse_hermes_model(&text);
        st.current_model = default;
        if let Some(name) = provider {
            st.current_base_url = hermes_provider_base(&text, &name);
        }
    }
    st.gateway_active = st
        .current_base_url
        .as_deref()
        .is_some_and(|u| is_gateway(u, port));
    st
}

fn detect_dsh() -> ToolStatus {
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

/// Cursor：主設定在 SQLite（state.vscdb）中，不做自動寫入，僅檢測。
fn detect_cursor() -> ToolStatus {
    let mut st = ToolStatus {
        app: "cursor".into(),
        display: "Cursor".into(),
        installed: on_path(&["cursor", "cursor-agent"]),
        config_path: None,
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: false,
    };
    if let Ok(home) = user_home() {
        // Windows: %APPDATA%\Cursor；macOS/Linux: ~/.cursor
        let win_cfg = dirs::data_dir().map(|d| d.join("Cursor")).unwrap_or_default();
        if home.join(".cursor").exists() || win_cfg.exists() {
            st.installed = true;
        }
    }
    st
}

/// Antigravity：CLI 設定 schema 未定，不做自動寫入，僅檢測。
fn detect_antigravity() -> ToolStatus {
    let mut st = ToolStatus {
        app: "antigravity".into(),
        display: "Antigravity".into(),
        installed: on_path(&["agy", "antigravity"]),
        config_path: None,
        current_base_url: None,
        current_model: None,
        gateway_active: false,
        supported_switch: false,
    };
    if let Ok(home) = user_home() {
        let cfg = home.join(".gemini").join("antigravity-cli").join("settings.json");
        if cfg.exists() {
            st.installed = true;
            st.config_path = Some(cfg.to_string_lossy().to_string());
        } else if home.join(".gemini").exists() {
            st.installed = true;
        }
    }
    st
}

pub fn detect_tools(port: u16) -> Vec<ToolStatus> {
    vec![
        detect_claude(port),
        detect_codex(port),
        detect_opencode(port),
        detect_hermes(port),
        detect_dsh(),
        detect_cursor(),
        detect_antigravity(),
    ]
}

// ---------------------------------------------------------------- 工具版本與更新 ---

/// 本機工具版本 + 一鍵更新資訊（版本頁用；與 10 秒輪詢的 detect 分開，避免頻繁起進程）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolVersion {
    pub app: String,
    pub display: String,
    pub installed: bool,
    /// 版本字串（取首行），未安裝或取不到時為 None。
    pub version: Option<String>,
    /// 更新命令 argv（不經 shell，直接執行；為空 = 不支援一鍵更新）。
    pub update_argv: Vec<String>,
    /// 更新命令的人類可讀展示。
    pub update_label: Option<String>,
    /// 是否已是最新（hermes 直接由 --version 輸出判定；npm 包由 tool_latest 填）。
    pub up_to_date: Option<bool>,
}

/// 最新版查詢結果（走 registry，有網絡延遲；前端獨立查詢，不擋版本顯示）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolLatest {
    pub app: String,
    pub latest: Option<String>,
    pub up_to_date: Option<bool>,
}

/// npm 全局包名（claude/codex/opencode/dsh 走 npm 更新）。
fn npm_package(app: &str) -> Option<&'static str> {
    match app {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "opencode" => Some("opencode-ai"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}

/// 在 PATH 中解析可執行檔完整路徑（Windows 含常見副檔名）。
fn which_bin(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for d in std::env::split_paths(&path) {
        for n in names {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
            #[cfg(windows)]
            for ext in ["exe", "cmd", "ps1"] {
                let q = d.join(format!("{n}.{ext}"));
                if q.is_file() {
                    return Some(q);
                }
            }
        }
    }
    None
}

/// 讀 npm 全局包 package.json 的 version（純讀檔，不起進程，最快最穩）。
fn npm_pkg_version_at(root: &Path, pkg: &str) -> Option<String> {
    // scoped 包 `@a/b` → node_modules\@a\b
    let rel: PathBuf = pkg.split('/').collect();
    let text = std::fs::read_to_string(root.join(rel).join("package.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("version")?
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(windows)]
fn npm_global_root() -> Option<PathBuf> {
    // npm 默認全局根 %APPDATA%\npm\node_modules；解析 npm.cmd 位置做二次確認。
    let appdata = dirs::data_dir()?;
    let root = appdata.join("npm").join("node_modules");
    if root.is_dir() {
        return Some(root);
    }
    which_bin(&["npm"]).and_then(|p| {
        p.parent()?
            .join("node_modules")
            .is_dir()
            .then(|| p.parent().expect("parent").join("node_modules"))
    })
}

/// 帶超時執行（輪詢等待，超時殺進程；不經 shell，argv[0] 必須是真 exe 或 cmd）。
fn run_with_timeout(argv: &[String], timeout_ms: u64) -> Result<String, String> {
    if argv.is_empty() {
        return Err("空命令".to_string());
    }
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW：避免彈黑框
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("啟動失敗：{e}"))?;
    let step = 200u64;
    let mut waited = 0u64;
    loop {
        match child.try_wait().map_err(|e| format!("等待失敗：{e}"))? {
            Some(status) => {
                if !status.success() {
                    return Err(format!("退出碼非零：{}", status.code().unwrap_or(-1)));
                }
                let out = child
                    .wait_with_output()
                    .map_err(|e| format!("讀輸出失敗：{e}"))?;
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stderr).to_string();
                }
                return Ok(text);
            }
            None => {
                if waited >= timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("超時（>{timeout_ms}ms），已終止"));
                }
                std::thread::sleep(std::time::Duration::from_millis(step));
                waited += step;
            }
        }
    }
}

/// 寬鬆捕獲（版本探測用）：不要求退出碼為零，只要有輸出就算數
///（部分 CLI 在有更新可用時 `--version` 非零退出）。
fn run_capture_lenient(argv: &[String], timeout_ms: u64) -> Option<String> {
    if argv.is_empty() {
        return None;
    }
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW：避免彈黑框
    }
    let mut child = cmd.spawn().ok()?;
    let step = 200u64;
    let mut waited = 0u64;
    loop {
        match child.try_wait().ok()? {
            Some(_) => {
                let out = child.wait_with_output().ok()?;
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stderr).to_string();
                }
                return if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                };
            }
            None => {
                if waited >= timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(step));
                waited += step;
            }
        }
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| {
            let mut t = l.to_string();
            if t.len() > 120 {
                t.truncate(120);
            }
            t
        })
}

/// 單工具版本探測（20 秒超時；npm 包優先讀 package.json）。
fn probe_tool_version(app: &str) -> (bool, Option<String>) {
    if let Some(pkg) = npm_package(app) {
        #[cfg(windows)]
        if let Some(root) = npm_global_root() {
            if let Some(v) = npm_pkg_version_at(&root, pkg) {
                return (true, Some(v));
            }
        }
        // 回退：跑二進制 --version（.ps1/.cmd 走 powershell/cmd）。
        if let Some(bin) = which_bin(&[app]) {
            let argv: Vec<String> = match bin.extension().and_then(|e| e.to_str()) {
                Some("ps1") => vec![
                    "powershell".to_string(),
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                    format!("& '{}' --version", bin.to_string_lossy()),
                ],
                Some("cmd") | Some("bat") => vec![
                    "cmd".to_string(),
                    "/C".to_string(),
                    bin.to_string_lossy().to_string(),
                    "--version".to_string(),
                ],
                _ => vec![bin.to_string_lossy().to_string(), "--version".to_string()],
            };
            if let Some(out) = run_capture_lenient(&argv, 20_000) {
                if let Some(v) = first_line(&out) {
                    return (true, Some(v));
                }
            }
            return (true, None);
        }
        return (false, None);
    }
    if app == "hermes" {
        // 獨立 exe（PATH 或 HERMES_HOME\bin），直接執行。
        let bin = which_bin(&["hermes"]).or_else(|| {
            let q = hermes_home().join("bin").join("hermes.exe");
            q.is_file().then_some(q)
        });
        if let Some(b) = bin {
            let argv = vec![b.to_string_lossy().to_string(), "--version".to_string()];
            if let Some(out) = run_capture_lenient(&argv, 20_000) {
                if let Some(v) = first_line(&out) {
                    return (true, Some(v));
                }
            }
            return (true, None);
        }
        // 僅有 config.yaml 也算"裝過"（detect 同口徑），但無二進制可問版本。
        return (hermes_home().join("config.yaml").exists(), None);
    }
    (false, None)
}

/// 單工具更新命令（argv 直接可執行；npm 走 `cmd /C npm …`，hermes 走自身 update）。
fn tool_update_argv(app: &str) -> (Vec<String>, Option<String>) {
    if let Some(pkg) = npm_package(app) {
        if which_bin(&[app]).is_none() {
            return (vec![], None);
        }
        return (
            vec![
                "cmd".to_string(),
                "/C".to_string(),
                "npm".to_string(),
                "i".to_string(),
                "-g".to_string(),
                format!("{pkg}@latest"),
            ],
            Some(format!("npm i -g {pkg}@latest")),
        );
    }
    if app == "hermes" {
        let bin = which_bin(&["hermes"]).or_else(|| {
            let q = hermes_home().join("bin").join("hermes.exe");
            q.is_file().then_some(q)
        });
        if let Some(b) = bin {
            return (
                vec![b.to_string_lossy().to_string(), "update".to_string()],
                Some("hermes update".to_string()),
            );
        }
    }
    (vec![], None)
}

/// hermes 是否已是最新：`--version` 輸出自帶 "Update available / behind" 提示。
/// 有提示 = false；無提示 = true；問不到 = None。
fn hermes_up_to_date() -> Option<bool> {
    let bin = which_bin(&["hermes"]).or_else(|| {
        let q = hermes_home().join("bin").join("hermes.exe");
        q.is_file().then_some(q)
    })?;
    let argv = vec![bin.to_string_lossy().to_string(), "--version".to_string()];
    let out = run_capture_lenient(&argv, 20_000)?;
    let low = out.to_lowercase();
    if low.contains("update available") || low.contains("behind") {
        Some(false)
    } else {
        Some(true)
    }
}

/// 版本段式比較（`1.2.10 > 1.2.9`；無後綴 > 有後綴，如 `0.1.2 > 0.1.2-rc.1`）。
fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |s: &str| {
        let (nums, pre) = match s.split_once('-') {
            Some((n, p)) => (n, Some(p.to_string())),
            None => (s, None),
        };
        let ns: Vec<u64> = nums.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        (ns, pre)
    };
    let (mut an, ap) = split(a.trim());
    let (mut bn, bp) = split(b.trim());
    let n = an.len().max(bn.len());
    an.resize(n, 0);
    bn.resize(n, 0);
    match an.cmp(&bn) {
        Ordering::Equal => match (ap, bp) {
            (None, None) => Ordering::Equal,
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(x), Some(y)) => x.cmp(&y),
        },
        ord => ord,
    }
}

/// 從人類可讀版本行提純番號（`codex-cli 0.153.4`→`0.153.4`，
/// `Hermes Agent v0.21.1 (2026.9.7)…`→`0.21.1`，`0.1.2-rc.1` 保留後綴）。
fn clean_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            if j < b.len() && b[j] == b'-' {
                let mut k = j + 1;
                while k < b.len()
                    && (b[k].is_ascii_alphanumeric() || b[k] == b'.' || b[k] == b'-')
                {
                    k += 1;
                }
                if k > j + 1 {
                    j = k;
                }
            }
            let cand = &s[i..j];
            if cand.contains('.') {
                return Some(cand.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

/// registry 最新版（`npm view <pkg> version`，30 秒超時；斷網/超時回 None）。
fn npm_latest(pkg: &str) -> Option<String> {
    let argv = vec![
        "cmd".to_string(),
        "/C".to_string(),
        "npm".to_string(),
        "view".to_string(),
        pkg.to_string(),
        "version".to_string(),
    ];
    let out = run_capture_lenient(&argv, 30_000)?;
    first_line(&out)
}

pub fn tool_versions() -> Vec<ToolVersion> {
    const APPS: [(&str, &str); 5] = [
        ("claude", "Claude Code"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("hermes", "Hermes Agent"),
        ("dsh", "DeepSeek Harness"),
    ];
    APPS.iter()
        .map(|(app, display)| {
            let (installed, version) = probe_tool_version(app);
            let (update_argv, update_label) = if installed {
                tool_update_argv(app)
            } else {
                (vec![], None)
            };
            let up_to_date = if *app == "hermes" && installed {
                hermes_up_to_date()
            } else {
                None
            };
            ToolVersion {
                app: app.to_string(),
                display: display.to_string(),
                installed,
                version,
                update_argv,
                update_label,
                up_to_date,
            }
        })
        .collect()
}

/// 各 npm 包最新版（並行查 registry；hermes 用本地輸出判定）。
/// 前端獨立調用，不擋版本顯示。
pub fn tool_latest() -> Vec<ToolLatest> {
    const NPM_APPS: [&str; 4] = ["claude", "codex", "opencode", "dsh"];
    // 本機已裝版本（讀 package.json，不起進程；缺失時回退跑 --version）。
    let mut cur: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    #[cfg(windows)]
    if let Some(root) = npm_global_root() {
        for app in NPM_APPS {
            if let Some(pkg) = npm_package(app) {
                if let Some(v) = npm_pkg_version_at(&root, pkg) {
                    cur.insert(app, v);
                }
            }
        }
    }
    for app in NPM_APPS {
        if !cur.contains_key(app) {
            if let (_, Some(v)) = probe_tool_version(app) {
                cur.insert(app, clean_version(&v).unwrap_or(v));
            }
        }
    }
    // 並行問 registry。
    let latest: std::collections::HashMap<&str, Option<String>> = std::thread::scope(|s| {
        let handles: Vec<(&str, std::thread::ScopedJoinHandle<Option<String>>)> = NPM_APPS
            .iter()
            .map(|app| {
                let pkg = npm_package(app).expect("npm app");
                (*app, s.spawn(move || npm_latest(pkg)))
            })
            .collect();
        handles
            .into_iter()
            .map(|(app, h)| (app, h.join().unwrap_or(None)))
            .collect()
    });
    let mut out: Vec<ToolLatest> = NPM_APPS
        .iter()
        .map(|app| {
            let l = latest.get(app).cloned().flatten();
            let up = match (cur.get(app), &l) {
                (Some(c), Some(n)) => {
                    Some(cmp_version(c, n) != std::cmp::Ordering::Less)
                }
                _ => None,
            };
            ToolLatest {
                app: app.to_string(),
                latest: l,
                up_to_date: up,
            }
        })
        .collect();
    out.push(ToolLatest {
        app: "hermes".to_string(),
        latest: None,
        up_to_date: hermes_up_to_date(),
    });
    out
}

/// 執行一鍵更新（600 秒超時；返回輸出尾部；npm/hermes 各走自家官方通道）。
pub fn tool_update_run(app: &str) -> Result<String, String> {
    let known = ["claude", "codex", "opencode", "hermes", "dsh"];
    if !known.contains(&app) {
        return Err(format!("未知工具：{app}"));
    }
    let (argv, label) = tool_update_argv(app);
    if argv.is_empty() {
        return Err("該工具未安裝或不支援一鍵更新".to_string());
    }
    let out = run_with_timeout(&argv, 600_000)?;
    // 只回輸出尾部（npm 日誌可達數百行）。
    let tail: String = out.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok(format!("{} 執行完畢：\n{tail}", label.unwrap_or_default()))
}

pub fn config_path_for(app: &str) -> Result<PathBuf, String> {
    match app {
        "claude" => Ok(user_home()?.join(".claude").join("settings.json")),
        "codex" => Ok(user_home()?.join(".codex").join("config.toml")),
        "opencode" => Ok(user_home()?
            .join(".config")
            .join("opencode")
            .join("opencode.json")),
        "hermes" => Ok(hermes_home().join("config.yaml")),
        "dsh" => Err("DSH 暫不支援自動寫入：endpoint 由 profile bundle 決定，M2 僅檢測 + 金鑰環境變量指引".into()),
        _ => Err(format!("未知工具：{app}")),
    }
}

// ---------------------------------------------------------------- 改寫 ---

/// Claude settings.json：合併 env，保留其他所有鍵。
/// map 非空檔位寫入 ANTHROPIC_MODEL / ANTHROPIC_DEFAULT_*（cc-switch 同款映射）。
pub fn claude_apply(
    existing: Option<&str>,
    base_url: &str,
    token: &str,
    map: Option<&ClaudeModelMap>,
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
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// 歷史別名上限（threads 去重後的 provider 名，防止失控膨脹）。
const CODEX_ALIAS_CAP: usize = 32;
/// provider 段內聯 models 上限（桌面版選擇器顯示用）。
const CODEX_INLINE_MODELS_CAP: usize = 200;

/// 文本級去重：丟棄第 2 個及之後重複的 `[model_providers.X]` 整段
///（多次接管 / 外部工具疊寫產生的非法 TOML，先消毒再解析；首段勝出）。
fn dedupe_codex_sections(text: &str) -> String {
    let mut out = String::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && !t.starts_with("[[") && t.ends_with(']') {
            let name = t[1..t.len() - 1].trim().to_string();
            if name == "model_providers" || name.starts_with("model_providers.") {
                if !seen.insert(name) {
                    skipping = true;
                    continue;
                }
            }
            skipping = false;
        }
        if skipping {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// 讀 Codex 歷史會話 provider 名的結果。
///
/// **為什麼需要這個型別**：原本的實作在讀不到時回傳空 `Vec`，與「真的沒有
/// 第三方 provider 殘留」完全無法區分。`codex_doctor()` 因此會在**讀取失敗
/// 時報「✅ 歷史會話無第三方 provider 殘留」** —— 把一個失敗報成通過。當
/// 使用者說「舊會話無法續用」時，唯一的診斷工具會告訴他一切正常，這是這個
/// 問題最難查的原因。
#[derive(Debug)]
pub enum LegacyProviders {
    /// 讀成功。`providers` 可能為空（＝真的沒有殘留）。
    Ok { providers: Vec<String>, db: PathBuf },
    /// 讀失敗，附可讀原因。**呼叫端不可把它靜默當成空。**
    Failed { reason: String },
}

/// 找出 Codex 狀態資料庫的候選，依版號由高到低。
///
/// **不可寫死 `state_5.sqlite`。** Codex 家目錄裡的檔名全部帶版本後綴
/// （`state_5`、`logs_2`、`goals_1`、`queue_1`、`memories_1`、
/// `thread_history_1`），那是它的 schema 版號。一旦 Codex 升版把 `state_5`
/// 換成 `state_6`，寫死路徑的程式就會**靜默回傳空 vec**，別名段全部消失，
/// 舊會話即無法續用 —— 而且是無聲無息地發生。這裡改成掃描所有
/// `state*.sqlite`。
fn codex_state_db_candidates(codex_home: &Path) -> Vec<PathBuf> {
    let mut cands: Vec<(u32, PathBuf)> = vec![];
    let Ok(rd) = std::fs::read_dir(codex_home) else {
        return vec![];
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("state") else {
            continue;
        };
        let Some(num) = rest.strip_suffix(".sqlite") else {
            continue;
        };
        // `state.sqlite` 視為 0、`state_5.sqlite` 為 5；其餘形式不認。
        let n: u32 = if num.is_empty() {
            0
        } else if let Some(d) = num.strip_prefix('_') {
            match d.parse() {
                Ok(v) => v,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        cands.push((n, e.path()));
    }
    cands.sort_by_key(|c| std::cmp::Reverse(c.0));
    cands.into_iter().map(|(_, p)| p).collect()
}

/// 讀單一 DB 的 provider 名；失敗時回傳**原因**（不再吞掉）。
fn read_legacy_providers_from(db: &Path) -> Result<Vec<String>, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("唯讀開啟失敗：{e}"))?;
    // Codex 隨時在寫這個檔（實測 `-wal` 可達 2 MB、`-shm` 一直在動）。
    // 沒有 busy_timeout 的話一遇鎖就直接失敗 —— 而失敗以前是靜默的。
    conn.busy_timeout(std::time::Duration::from_millis(3000))
        .map_err(|e| format!("設定 busy_timeout 失敗：{e}"))?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT model_provider FROM threads")
        .map_err(|e| format!("沒有 threads 表或查詢無法準備：{e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("查詢失敗：{e}"))?;
    let mut out: Vec<String> = vec![];
    let mut row_errs = 0usize;
    for r in rows {
        // 以前是 `rows.flatten()`，這一類錯誤被整個吞掉（第 5 條靜默路徑）。
        let id = match r {
            Ok(v) => v.trim().to_string(),
            Err(_) => {
                row_errs += 1;
                continue;
            }
        };
        if id.is_empty() || id == CODEX_SHARED_PROVIDER_ID || id == GATEWAY_PROVIDER_ID {
            continue;
        }
        // 僅保留合法 TOML 鍵字元
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        if !out.contains(&id) {
            out.push(id);
        }
        if out.len() >= CODEX_ALIAS_CAP {
            break;
        }
    }
    // 有列但全讀不出來 → 這是失敗，不是「沒有殘留」。
    if out.is_empty() && row_errs > 0 {
        return Err(format!("{row_errs} 列的 model_provider 都讀不出來"));
    }
    Ok(out)
}

/// 讀 Codex 歷史會話用過的 provider 名，並**明確區分成功與失敗**。
///
/// 只讀不寫。逐個候選 DB（版號高者優先）嘗試，第一個成功者勝出；全部失敗
/// 時回傳 [`LegacyProviders::Failed`] 並附完整原因。
pub fn codex_legacy_providers_report(codex_home: &Path) -> LegacyProviders {
    let cands = codex_state_db_candidates(codex_home);
    if cands.is_empty() {
        return LegacyProviders::Failed {
            reason: format!(
                "在 {} 找不到任何 state*.sqlite（Codex 尚未產生，或改了檔名規則）",
                codex_home.display()
            ),
        };
    }
    let mut why: Vec<String> = vec![];
    for db in &cands {
        match read_legacy_providers_from(db) {
            Ok(providers) => {
                return LegacyProviders::Ok {
                    providers,
                    db: db.clone(),
                }
            }
            Err(e) => why.push(format!(
                "{}：{e}",
                db.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    LegacyProviders::Failed {
        reason: format!(
            "找到 {} 個 state*.sqlite 但都讀不出 provider：{}",
            cands.len(),
            why.join("；")
        ),
    }
}

/// 要管理的別名段名 = （DB 讀到的 provider 名）∪（config.toml 裡**已經指向
/// 本網關**的 `[model_providers.*]` 段名）。
///
/// **為什麼要聯集**：`codex_apply` 只會刪除「在管理清單裡」的段。若清單只來自
/// DB，一旦 threads 變少（例如使用者**封存對話**之後，那個 provider 不再出現
/// 在 `SELECT DISTINCT model_provider` 裡），本來存在的別名段就會被剔除，
/// 舊會話的 provider 段因而消失。聯集讓已由本工具接管的段**只增不減**，這正
/// 是「舊會話必須一直能續用」所需要的性質。
///
/// **只納入 base_url 已經指向本網關的段**，不碰使用者自己指向上游的 provider
/// —— 否則接管會把「使用者想直連」的段也一併劫持。
fn codex_alias_ids(existing: &str, from_db: &[String], gw_url: &str) -> Vec<String> {
    let mut out: Vec<String> = from_db.to_vec();
    let clean = dedupe_codex_sections(existing);
    let Ok(doc) = clean.parse::<toml_edit::DocumentMut>() else {
        return out;
    };
    let Some(mp) = doc.get("model_providers").and_then(|m| m.as_table()) else {
        return out;
    };
    for (k, v) in mp.iter() {
        let id = k.trim();
        if id.is_empty()
            || id == CODEX_SHARED_PROVIDER_ID
            || id == GATEWAY_PROVIDER_ID
            || out.iter().any(|o| o == id)
        {
            continue;
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        // 只接管本來就指向本網關的段。
        if v.get("base_url").and_then(|b| b.as_str()) != Some(gw_url) {
            continue;
        }
        out.push(id.to_string());
    }
    out
}

/// 從已生成的 catalog 讀 (slug, display_name)，寫入 provider 段內聯 models（桌面版選擇器顯示用）。
/// catalog 缺失 / 解析失敗時返回空（不阻塞接管）。
pub fn codex_inline_models(catalog: Option<&str>) -> Vec<(String, String)> {
    let path = match catalog {
        Some(c) if !c.trim().is_empty() => c,
        _ => return vec![],
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v.get("models").and_then(|m| m.as_array()) {
        Some(a) => a,
        None => return vec![],
    };
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for e in arr {
        let slug = e
            .get("slug")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
            continue;
        }
        let display = e
            .get("display_name")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim();
        let name = if display.is_empty() {
            slug.clone()
        } else {
            display.to_string()
        };
        out.push((slug, name));
        if out.len() >= CODEX_INLINE_MODELS_CAP {
            break;
        }
    }
    out
}

/// 這個 `base_url` 該用哪種 `wire_api`（Codex 送出時的協議形狀）。
///
/// **為什麼不能無差別寫 `responses`**：`wire_api` 決定 Codex 用哪種協議打
/// `base_url` —— `responses` 走 `/v1/responses`、`chat` 走
/// `/v1/chat/completions`。本網關**兩種都收**並代為轉譯，所以指向網關時用
/// `responses` 沒問題（那也是 Codex 的原生形狀，能保留 reasoning 等欄位）。
/// 但**直連第三方**時 `base_url` 是對方（`Providers.tsx` 的
/// `base_url: via ? gatewayUrl(...) : provider.base_url`），而多數第三方只
/// 實作 Chat Completions —— 硬寫 `responses` 會讓 Codex 打到不存在的端點而
/// 404。
///
/// 這個問題原本**已經被發現過**（`restore_backup` 的提示文字寫著「該配置
/// 直連第三方 URL 但走 responses 協議（直連 Chat 上游會 404）」），但當時只
/// **警告、不修**。這裡改成寫入時就選對，讓那個警告不再需要出現。
pub fn codex_wire_api(base_url: &str) -> &'static str {
    let u = base_url.trim().to_ascii_lowercase();
    // 指向本網關：兩種協議都收，用 Codex 原生的 responses（保留 reasoning 等）。
    if u.contains(GATEWAY_HOST) {
        return "responses";
    }
    // 官方 OpenAI：兩種都支援，維持 responses。
    if u.contains("api.openai.com") {
        return "responses";
    }
    // 其餘第三方：Chat Completions 是唯一普遍實作的形狀。
    "chat"
}

fn gateway_section(
    base_url: &str,
    inline_models: &[(String, String)],
    auth: CodexAuth<'_>,
) -> toml_edit::Table {
    let mut tbl = toml_edit::Table::new();
    tbl["name"] = toml_edit::value("Token Gateway");
    tbl["base_url"] = toml_edit::value(base_url);
    match auth.direct_key {
        // 直連上游：Bearer 即上游 Key，明文寫入（用戶顯式選擇，見警告）
        Some(k) => {
            tbl["experimental_bearer_token"] = toml_edit::value(k);
        }
        None => {
            tbl["env_key"] = toml_edit::value(GATEWAY_ENV_KEY);
        }
    }
    // 協議形狀：**優先採用 provider 的明確宣告**（`api_format`），只有在沒有
    // 宣告時才依 base_url 推定。宣告比猜準（第三方也可能提供 responses 端點）。
    tbl["wire_api"] = toml_edit::value(auth.wire_api.unwrap_or_else(|| codex_wire_api(base_url)));
    if !inline_models.is_empty() {
        let mut arr = toml_edit::Array::new();
        for (m, display) in inline_models {
            let mut item = toml_edit::InlineTable::new();
            item.insert("model", toml_edit::Value::from(m.clone()));
            item.insert("display_name", toml_edit::Value::from(display.clone()));
            arr.push(toml_edit::Value::InlineTable(item));
        }
        tbl["models"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
    }
    tbl
}

/// Codex config.toml：toml_edit 手術式改寫，保留註釋與無關段。
/// reasoning 僅在顯式設定（非 unset）時寫入；catalog 為模型目錄路徑時寫入。
/// 共享段名（cc-switch 同款）：`model_provider = "custom"`，custom 與
/// tokengateway 兩段同時指向網關，外加歷史會話用過的 provider 別名段——
/// cc-switch 時代與網關時代的舊會話（按段名引用供應商）都能繼續，
/// 段被外部工具刪除也會在下次接管時重建。
/// 冪等：先刪後插＋回驗，多次接管不疊段；入口先做重複段消毒（非法 TOML 也能救）。
/// [`codex_apply`] 的認證與協議選項。
///
/// 把這兩個欄位綁在一起，是為了讓參數列維持在 8 個（clippy
/// `too_many_arguments` 門檻）—— 原本只有 `direct_key`，加入協議宣告後若
/// 直接再多一個參數就會超標。
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAuth<'a> {
    /// 直連上游 Key（Some = 直連模式：各段寫 `experimental_bearer_token`
    /// 明文，不寫 `env_key`）。
    pub direct_key: Option<&'a str>,
    /// 協議形狀覆寫，來自 provider **宣告**的 `api_format`（§5.3 能力宣告）。
    /// `None` = 依 `base_url` 推定（見 [`codex_wire_api`]）。
    pub wire_api: Option<&'a str>,
}

/// 依 provider 宣告的 `api_format` 決定 Codex 該用哪種 `wire_api`。
///
/// 這是「能力宣告優先於事後猜測」的具體落點：`providers.api_format` 是使用者
/// 對該渠道的**明確宣告**，比從 URL 猜準確（第三方也可能提供 responses
/// 端點）。
///
/// 回傳 `None` = 宣告不足以判定（未宣告，或 Anthropic／Gemini 這種 Codex
/// 根本說不了的協議），交由 [`codex_wire_api`] 依 URL 推定接手。
pub fn codex_wire_api_declared(provider_format: Option<&str>) -> Option<&'static str> {
    match provider_format?.trim() {
        // 只實作 Chat Completions
        "openai-chat" => Some("chat"),
        // 兩種都支援 → 用 Codex 原生的 responses（保留 reasoning 等欄位）
        "mixed" | "openai-responses" => Some("responses"),
        _ => None,
    }
}

pub fn codex_apply(
    existing: &str,
    base_url: &str,
    model: &str,
    reasoning: Option<&str>,
    catalog: Option<&str>,
    aliases: &[String],
    inline_models: &[(String, String)],
    auth: CodexAuth<'_>,
) -> Result<String, String> {
    let clean = dedupe_codex_sections(existing);
    let mut doc: toml_edit::DocumentMut = clean
        .parse()
        .map_err(|e| format!("config.toml 解析失敗：{e}"))?;
    doc["model"] = toml_edit::value(model);
    doc["model_provider"] = toml_edit::value(CODEX_SHARED_PROVIDER_ID);
    if let Some(r) = reasoning {
        if !r.trim().is_empty() && r != "unset" {
            doc["model_reasoning_effort"] = toml_edit::value(r);
        }
    }
    if let Some(c) = catalog {
        if !c.trim().is_empty() {
            doc["model_catalog_json"] = toml_edit::value(c);
        }
    }
    // 顯式建表，保證輸出為 `[model_providers.xxx]` 段頭形式
    //（鏈式索引缺失鍵時會退化成內聯表寫法；注意 Index 讀缺失鍵會 panic，一律用 get）。
    if doc.get("model_providers").is_none() {
        doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let mp = doc["model_providers"]
        .as_table_mut()
        .ok_or("model_providers 不是表，無法接管")?;
    // 先刪後插：冪等，多次接管不疊段
    let mut managed: Vec<String> = vec![
        CODEX_SHARED_PROVIDER_ID.to_string(),
        GATEWAY_PROVIDER_ID.to_string(),
    ];
    for a in aliases {
        let id = a.trim();
        if id.is_empty() || managed.iter().any(|m| m == id) {
            continue;
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        managed.push(id.to_string());
        if managed.len() >= CODEX_ALIAS_CAP + 2 {
            break;
        }
    }
    for id in &managed {
        mp.remove(id.as_str());
    }
    for id in &managed {
        mp.insert(
            id.as_str(),
            toml_edit::Item::Table(gateway_section(base_url, inline_models, auth)),
        );
    }
    let out = doc.to_string();
    // 回驗：託管段頭恰好出現一次，且整檔可再解析；異常則拒絕寫入
    for id in &managed {
        let header = format!("[model_providers.{id}]");
        let n = out.lines().filter(|l| l.trim() == header).count();
        if n != 1 {
            return Err(format!("接管後 {header} 出現 {n} 次，已拒絕寫入"));
        }
    }
    out.parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("接管結果校驗失敗：{e}"))?;
    Ok(out)
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
        serde_json::Value::String("Token Gateway".to_string()),
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
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// 在 `section:` 段內設定 2 縮進標量；段內缺失則追加到段尾（段最後一個條目之後）。
fn set_yaml_scalar(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("{section}:");
    let key_prefix = format!("{key}:");
    let lines: Vec<&str> = text.lines().collect();
    // 找到段頭
    let mut head_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if line.len() == t.len() && t == header {
            head_idx = Some(i);
            break;
        }
    }
    let Some(hi) = head_idx else {
        // 整段缺失：末尾追加
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&format!("{header}\n  {key}: {value}\n"));
        return s;
    };
    // 段尾 = 下一個 0 縮進非空非註釋行之前
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(hi + 1) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if line.len() == t.len() {
            end = i;
            break;
        }
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    let mut changed = false;
    for (i, line) in lines.iter().enumerate() {
        if i > hi && i < end && !changed {
            let t = line.trim_start();
            let indent = line.len() - t.len();
            if indent == 2 && (t == key_prefix || t.starts_with(&format!("{key_prefix} "))) {
                out.push(format!("  {key}: {value}"));
                changed = true;
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !changed {
        // 追加到段尾（end 之前，跳過尾部空行/註釋行之上？直接插在 end 處即可）
        out.insert(end, format!("  {key}: {value}"));
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// 確保 hermes `providers.tokengateway` 塊存在且 base_url / model / key_env 正確。
/// key_env 是 hermes 具名自訂 provider 的憑證指針（憑證本體在 .env），
/// 缺了它 hermes 會用 "no-key-required" 佔位 key → 網關 401。
fn ensure_hermes_provider(text: &str, base_url: &str, model: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut prov_head = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if line.len() == t.len() && (t == "providers:" ) {
            prov_head = Some(i);
            break;
        }
    }
    let block = [
        format!("  {GATEWAY_PROVIDER_ID}:"),
        "    name: tokengateway".to_string(),
        format!("    base_url: {base_url}"),
        format!("    model: {model}"),
        format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"),
        "    extra_headers:".to_string(),
        "      X-TG-App: hermes".to_string(),
        "    discover_models: false".to_string(),
    ];
    let Some(phi) = prov_head else {
        let mut s = text.to_string();
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str("providers:\n");
        s.push_str(&block.join("\n"));
        s.push('\n');
        return s;
    };
    // 段尾
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(phi + 1) {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if line.len() == t.len() {
            end = i;
            break;
        }
    }
    // 找現有 tokengateway 塊
    let mut block_idx = None;
    for (i, line) in lines.iter().enumerate().skip(phi + 1).take(end - (phi + 1)) {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        if indent == 2 && t.trim_end_matches(':') == GATEWAY_PROVIDER_ID && t.ends_with(':') {
            block_idx = Some(i);
            break;
        }
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    if let Some(bi) = block_idx {
        // 塊範圍：bi+1 起、直到縮進 <= 2 的非空非註釋行為止
        let mut block_end = out.len();
        for (i, line) in out.iter().enumerate().skip(bi + 1) {
            let t = line.trim_start();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if line.len() - t.len() <= 2 {
                block_end = i;
                break;
            }
        }
        // 逐欄位定位（4 縮進）：base_url / model / key_env / extra_headers
        let (mut base_idx, mut model_idx, mut key_idx, mut eh_idx) = (None, None, None, None);
        for (i, line) in out.iter().enumerate().take(block_end).skip(bi + 1) {
            let t = line.trim_start();
            if line.len() - t.len() != 4 {
                continue;
            }
            if t == "base_url:" || t.starts_with("base_url: ") {
                base_idx = Some(i);
            } else if t == "model:" || t.starts_with("model: ") {
                model_idx = Some(i);
            } else if t == "key_env:" || t.starts_with("key_env: ") {
                key_idx = Some(i);
            } else if t == "extra_headers:" {
                eh_idx = Some(i);
            }
        }
        if let Some(i) = base_idx {
            out[i] = format!("    base_url: {base_url}");
        }
        if let Some(i) = model_idx {
            out[i] = format!("    model: {model}");
        }
        match key_idx {
            Some(i) => out[i] = format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"),
            None => {
                let at = model_idx
                    .or(base_idx)
                    .map(|i| i + 1)
                    .unwrap_or(block_end)
                    .min(out.len());
                out.insert(at, format!("    key_env: {HERMES_CUSTOM_ENV_KEY}"));
            }
        }
        if model_idx.is_none() {
            let at = base_idx.map(|i| i + 1).unwrap_or(block_end).min(out.len());
            out.insert(at, format!("    model: {model}"));
        }
        // X-TG-App 標頭：網關據此把用量歸到 hermes（否則 UA 是 OpenAI/Python → unknown）
        if eh_idx.is_none() {
            let at = key_idx
                .map(|i| i + 1)
                .or(model_idx.map(|i| i + 1))
                .or(base_idx.map(|i| i + 1))
                .unwrap_or(block_end)
                .min(out.len());
            out.insert(at, "      X-TG-App: hermes".to_string());
            out.insert(at, "    extra_headers:".to_string());
        }
    } else {
        let insert_at = end.min(out.len());
        for (k, b) in block.iter().enumerate() {
            out.insert(insert_at + k, b.clone());
        }
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

pub fn hermes_apply(existing: &str, base_url: &str, model: &str) -> Result<String, String> {
    let t = ensure_hermes_provider(existing, base_url, model);
    let t = set_yaml_scalar(&t, "model", "provider", GATEWAY_PROVIDER_ID);
    let t = set_yaml_scalar(&t, "model", "default", model);
    Ok(t)
}

/// `.env` 行式 replace-or-append（保留註釋與空行）。
pub fn dotenv_set(text: &str, key: &str, value: &str) -> String {
    let mut out = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let t = line.trim_start();
        if !changed && !t.starts_with('#') && (t == key || t.starts_with(&format!("{key}="))) {
            out.push(format!("{key}={value}"));
            changed = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !changed {
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') || text.is_empty() {
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------- 方案 ---

#[derive(Debug, Clone, Deserialize)]
pub struct SwitchRequest {
    pub app: String,
    /// 已解析好的目標 base_url（網關 URL 或上游直連 URL）。
    pub base_url: String,
    /// 要使用的密鑰取值：網關模式=本地 sk key，直連=上游 key。僅 codex/opencode/dsh 用於生成
    /// 環境變量指引；claude 寫入文件（見警告）；hermes 寫入自家 .env。
    pub api_key: String,
    pub model: String,
    pub via_gateway: bool,
    /// 選中的渠道（產生 Codex 目錄時必填）。
    pub provider_id: Option<i64>,
    /// 選中渠道的格式（計劃頁做相容提示用）。
    pub provider_format: Option<String>,
    /// 選中模型的思考等級（unset/None = 不動配置文件現值）。
    pub reasoning: Option<String>,
    /// 選中模型的上下文視窗（None = 不寫；僅 opencode/Codex 目錄生效）。
    pub context_window: Option<i64>,
    /// 是否同時產生 Codex 模型目錄（僅 codex 有效）。
    pub gen_catalog: bool,
    /// Codex 目錄是否包含其他來源模型（聯合目錄；默认僅當前來源）。
    #[serde(default)]
    pub catalog_union: bool,
    /// 直連上游模式（Key 欄留空時置 true）：後端以來源的上游 Key 直連
    /// （經網關=不經本地 Key 轉發；直連=直接寫入工具配置）；用量照常記錄
    /// （僅不計本地 Key 配額）、僅允許該來源。
    #[serde(default)]
    pub direct_upstream: bool,
    /// 前端下拉記憶的本地 Key id（後端以 api_key 為準，此欄僅回填選項用）。
    #[serde(default)]
    pub key_id: Option<i64>,
    /// Claude Code 模型映射（預設/haiku/sonnet/opus/fable；
    /// 寫入 settings.json 的 ANTHROPIC_MODEL / ANTHROPIC_DEFAULT_*；僅 claude 有效）。
    #[serde(default)]
    pub claude_map: Option<ClaudeModelMap>,
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
    fn pairs(&self) -> [(&'static str, &str); 5] {
        [
            ("ANTHROPIC_MODEL", self.default.as_str()),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL", self.haiku.as_str()),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", self.sonnet.as_str()),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", self.opus.as_str()),
            ("ANTHROPIC_DEFAULT_FABLE_MODEL", self.fable.as_str()),
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchPlan {
    pub app: String,
    pub config_path: String,
    pub supported: bool,
    pub will_backup: bool,
    pub edits: Vec<String>,
    pub env_setup: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchResult {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub extra_files: Vec<String>,
}

/// 工具原生格式（計劃頁相容提示用）。
fn tool_format(app: &str) -> &'static str {
    match app {
        "claude" => "anthropic",
        "codex" => "openai-responses",
        _ => "openai-chat",
    }
}

fn ps_export(var: &str) -> String {
    format!("$env:{var} = \"<把密鑰貼在這裡>\"  # 當前終端生效；長期使用請設為用戶環境變量")
}

pub fn plan_switch(req: &SwitchRequest, port: u16) -> Result<SwitchPlan, String> {
    if !APPS.iter().any(|(a, _)| *a == req.app) {
        return Err(format!("未知工具：{}", req.app));
    }
    if req.model.trim().is_empty() {
        return Err("model 不能為空".to_string());
    }
    let mut normalized = req.clone();
    if normalized.via_gateway {
        normalized.base_url = gateway_url(port, &normalized.app);
    }
    let req = &normalized;
    let path = config_path_for(&req.app);
    let gw = req.via_gateway;
    let mut plan = SwitchPlan {
        app: req.app.clone(),
        config_path: path
            .as_deref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        supported: true,
        will_backup: path.as_ref().is_ok_and(|p| p.exists()),
        edits: vec![],
        env_setup: vec![],
        warnings: vec![],
    };
    // 通用警告：正在被本地代理接管的配置將被替換
    for st in detect_tools(port) {
        if st.app == req.app && st.gateway_active && !gw {
            plan.warnings.push(
                "檢測到該工具當前正被本地代理接管（疑似 cc-switch），切換將替換其 base_url（已自動備份，可回滾）"
                    .to_string(),
            );
        }
    }
    match req.app.as_str() {
        "claude" => {
            plan.edits.push(format!("env.ANTHROPIC_BASE_URL = {}", req.base_url));
            plan.edits.push("env.ANTHROPIC_AUTH_TOKEN = ***（明文寫入 settings.json）".into());
            if let Some(m) = req.claude_map.as_ref() {
                for (k, v) in m.pairs() {
                    let v = v.trim();
                    if !v.is_empty() {
                        plan.edits.push(format!("env.{k} = {v}"));
                    }
                }
            }
            if req.via_gateway {
                plan.warnings.push(
                    "ANTHROPIC_AUTH_TOKEN 將明文寫入 settings.json：網關模式下請使用可吊銷的本地 sk key，不要放上游長效 key"
                        .to_string(),
                );
            }
        }
        "codex" => {
            plan.edits.push(format!("model = {}", req.model));
            plan.edits.push(format!(
                "model_provider = {CODEX_SHARED_PROVIDER_ID}（共享段名，custom / tokengateway / 歷史別名段同時指向網關，舊會話可繼續）"
            ));
            // 預覽歷史別名。刻意用 report 版：**讀失敗要在接管前就讓使用者
            // 看到**，而不是等到舊會話失效、回頭懷疑是網關弄壞了什麼。
            let aliases = match path
                .as_ref()
                .ok()
                .and_then(|p| p.parent())
                .map(codex_legacy_providers_report)
            {
                Some(LegacyProviders::Ok { providers, .. }) => providers,
                Some(LegacyProviders::Failed { reason }) => {
                    plan.warnings.push(format!(
                        "讀不到 Codex 歷史會話的 provider 名：{reason}。接管可能無法補寫別名段，\
                         仍在使用舊會話的 provider 可能失效（請先完全結束 Codex 再接管）"
                    ));
                    vec![]
                }
                None => vec![],
            };
            let auth_desc = if req.direct_upstream {
                "experimental_bearer_token = 上游 Key 明文"
            } else {
                "env_key = TOKEN_GATEWAY_KEY"
            };
            // 協議形狀由 provider 宣告（`api_format`）優先、URL 推定為後備；
            // 預覽必須顯示**實際會寫入的值**，否則預覽會騙人。
            let wire_api = codex_wire_api_declared(req.provider_format.as_deref())
                .unwrap_or_else(|| codex_wire_api(&req.base_url));
            if aliases.is_empty() {
                plan.edits.push(format!(
                    "[model_providers.{CODEX_SHARED_PROVIDER_ID}] 與 [model_providers.{GATEWAY_PROVIDER_ID}] base_url = {} / {auth_desc} / wire_api = {wire_api}",
                    req.base_url
                ));
            } else {
                plan.edits.push(format!(
                    "[model_providers.{{custom, tokengateway{}}}] 共 {} 段 base_url = {} / {auth_desc} / wire_api = {wire_api}",
                    if aliases.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", aliases.join(", "))
                    },
                    aliases.len() + 2,
                    req.base_url
                ));
            }
            plan.edits.push(
                "provider 段內聯 models（桌面版選擇器顯示用）＋接管結果回驗（託管段恰好一次，異常拒寫）"
                    .to_string(),
            );
            if let Some(r) = req.reasoning.as_deref() {
                if !r.trim().is_empty() && r != "unset" {
                    plan.edits.push(format!("model_reasoning_effort = {r}"));
                }
            }
            if req.gen_catalog {
                plan.edits.push(format!(
                    "按可用模型表產生 Codex 模型目錄（{}）並寫入 model_catalog_json",
                    if req.catalog_union {
                        "聯合：含其他啟用來源模型"
                    } else {
                        "僅當前來源模型"
                    }
                ));
            }
            if req.direct_upstream {
                plan.edits.push(
                    "直連模式：各段寫 experimental_bearer_token = 上游 Key 明文（不寫 env_key）"
                        .to_string(),
                );
                plan.warnings.push(if req.via_gateway {
                    "直連模式：上游 Key 明文寫入 config.toml，僅本機可讀；流量仍經網關轉發，用量照常記錄（僅不計本地 Key 配額）、僅允許該來源登記的模型、不做跨來源路由"
                        .to_string()
                } else {
                    "直連模式：上游 Key 明文寫入 config.toml，僅本機可讀；流量不經網關，無用量審計與故障轉移"
                        .to_string()
                });
            } else {
                plan.env_setup.push(ps_export(GATEWAY_ENV_KEY));
                plan.warnings.push(
                    "金鑰不寫入檔案：Codex 官方要求經 env_key 引用的環境變量提供（experimental_bearer_token 已被官方明確不鼓勵）"
                        .to_string(),
                );
            }
        }
        "opencode" => {
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.options.baseURL = {}",
                req.base_url
            ));
            let apikey_desc = if req.direct_upstream {
                "上游 Key 明文（直連模式）".to_string()
            } else {
                format!("{{env:{GATEWAY_ENV_KEY}}}（引用，不落地）")
            };
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.options.apiKey = {apikey_desc}",
            ));
            plan.edits.push(format!(
                "provider.{GATEWAY_PROVIDER_ID}.models 登記 {}",
                req.model
            ));
            if let Some(cw) = req.context_window {
                if cw > 0 {
                    plan.edits.push(format!(
                        "provider.{GATEWAY_PROVIDER_ID}.models.{}.limit.context = {cw}（上文上限）",
                        req.model
                    ));
                }
            }
            if req.direct_upstream {
                plan.warnings.push(if req.via_gateway {
                    "直連模式：上游 Key 明文寫入 opencode.json，僅本機可讀；流量仍經網關轉發，用量照常記錄（僅不計本地 Key 配額）、僅允許該來源登記的模型、不做跨來源路由"
                        .to_string()
                } else {
                    "直連模式：上游 Key 明文寫入 opencode.json，僅本機可讀；流量不經網關，無用量審計與故障轉移"
                        .to_string()
                });
            } else {
                plan.env_setup.push(ps_export(GATEWAY_ENV_KEY));
            }
            if !req.direct_upstream {
                plan.warnings.push(
                    "變量必須存在於 opencode 行程環境中：請在啟動 opencode 的同一終端機先執行上面命令".to_string(),
                );
            }
            plan.warnings.push(format!(
                "切換後需在 opencode 內手動選中模型 {GATEWAY_PROVIDER_ID}/{}",
                req.model
            ));
        }
        "hermes" => {
            plan.edits.push(format!(
                "providers.{GATEWAY_PROVIDER_ID}.base_url = {}",
                req.base_url
            ));
            plan.edits.push(format!("model.provider = {GATEWAY_PROVIDER_ID}"));
            plan.edits.push(format!("model.default = {}", req.model));
            plan.edits.push(format!(
                "providers.{GATEWAY_PROVIDER_ID}.key_env = {HERMES_CUSTOM_ENV_KEY}（憑證指針，hermes 據此讀 .env）"
            ));
            plan.edits.push(format!(
                ".env 寫入 {HERMES_CUSTOM_ENV_KEY}=***（復刻其 flatkey 自有約定）"
            ));
        }
        "dsh" => {
            plan.supported = false;
            plan.will_backup = false;
            plan.env_setup
                .push("$env:DEEPSEEK_API_KEY = \"<把金鑰粘貼在這裡>\"".to_string());
            plan.warnings.push(
                "DSH 的 endpoint 由 profile bundle（deepseek-official）決定，M2 僅管理 DEEPSEEK_API_KEY，網關接管待 M3 驗證"
                    .to_string(),
            );
            return Ok(plan);
        }
        "cursor" => {
            plan.supported = false;
            plan.will_backup = false;
            plan.warnings.push(
                "Cursor 主設定在 SQLite（state.vscdb），請手工在 設定 → Models 中填 OpenAI API Key 並勾選 Override OpenAI Base URL 指向本網關"
                    .to_string(),
            );
            return Ok(plan);
        }
        "antigravity" => {
            plan.supported = false;
            plan.will_backup = false;
            plan.warnings.push(
                "Antigravity CLI 設定格式未定，請手工配置 ~/.gemini/antigravity-cli/settings.json 指向本網關"
                    .to_string(),
            );
            return Ok(plan);
        }
        _ => unreachable!(),
    }
    if !req.via_gateway {
        plan.warnings.push(
            "直連模式：base_url 直接指向第三方，流量不經過本網關，無用量審計與故障轉移".to_string(),
        );
    }
    // 格式相容提示（網關直連模式不經網關，此處僅提示經網關時的行為）
    if req.via_gateway {
        if let Some(pf) = req.provider_format.as_deref() {
            match (tool_format(&req.app), pf) {
                ("anthropic", "openai-chat") | ("anthropic", "mixed") => {
                    plan.edits.push(
                        "將自動做 Anthropic→OpenAI 轉換（模型按可用模型表映射，無映射原樣透傳）"
                            .to_string(),
                    );
                }
                ("anthropic", "anthropic") => {
                    plan.edits.push("Anthropic 原生透傳".to_string());
                }
                (_, "anthropic") => {
                    plan.warnings.push(
                        "格式不相容：該工具不說 Anthropic，請求會被網關拒絕（反向轉換未實現）"
                            .to_string(),
                    );
                }
                _ => {}
            }
        }
    }
    Ok(plan)
}

pub fn apply_switch(
    app_data: &Path,
    mut req: SwitchRequest,
    port: u16,
    catalog: Option<String>,
) -> Result<SwitchResult, String> {
    if req.via_gateway {
        req.base_url = gateway_url(port, &req.app);
    }
    // 先出方案做合法性校驗（dsh 等直接在此拒絕）。
    let plan = plan_switch(&req, port)?;
    if !plan.supported {
        return Err("該工具暫不支持自動寫入，請按 env_setup 指引手工配置".to_string());
    }
    let backups_root = app_data.join("backups");
    let cfg = config_path_for(&req.app)?;
    let existing = read_text(&cfg);
    let reasoning = req.reasoning.as_deref();
    let new_text = match req.app.as_str() {
        "claude" => {
            claude_apply(existing.as_deref(), &req.base_url, &req.api_key, req.claude_map.as_ref())?
        }
        "codex" => {
            // 歷史會話用過的 provider 名全寫為網關別名段（只讀 threads，不寫 DB）。
            //
            // 再與 config.toml 裡**已指向本網關**的別名段聯集，讓管理清單只增
            // 不減：若清單只來自 DB，一旦 threads 變少（例如使用者**封存對話**
            // 之後該 provider 不再出現在 `SELECT DISTINCT model_provider`），
            // 舊的別名段就會被 `codex_apply` 剔除，舊會話的 provider 段因而
            // 消失 —— 這正是「舊會話無法續用／無法封存」的可能成因。
            let gw_url = gateway_url(port, "codex");
            let db_aliases = match cfg.parent().map(codex_legacy_providers_report) {
                Some(LegacyProviders::Ok { providers, .. }) => providers,
                // 讀失敗時 `codex_alias_ids` 仍會保住既有的網關別名段。
                _ => vec![],
            };
            let aliases = codex_alias_ids(existing.as_deref().unwrap_or(""), &db_aliases, &gw_url);
            let inline_models = codex_inline_models(catalog.as_deref());
            // 直連上游：api_key 即 switch_apply 命令層注入的上游 Key（明文寫段內）
            let direct_key = if req.direct_upstream && !req.api_key.trim().is_empty() {
                Some(req.api_key.trim())
            } else {
                None
            };
            codex_apply(
                existing.as_deref().unwrap_or(""),
                &req.base_url,
                &req.model,
                reasoning,
                catalog.as_deref(),
                &aliases,
                &inline_models,
                // 協議形狀優先採用 provider 的明確宣告（`api_format`），
                // 沒有宣告時 `gateway_section` 才依 base_url 推定。
                // 這正是 §5.3 第 2 層「能力宣告」的落點。
                CodexAuth {
                    direct_key,
                    wire_api: codex_wire_api_declared(req.provider_format.as_deref()),
                },
            )?
        }
        "opencode" => {
            // 直連上游：api_key 即 switch_apply 命令層注入的上游 Key（明文寫 options.apiKey，
            // 與 Codex 的 experimental_bearer_token 對等；缺省仍走 {env:} 引用）。
            let direct_key = if req.direct_upstream && !req.api_key.trim().is_empty() {
                Some(req.api_key.trim())
            } else {
                None
            };
            opencode_apply(existing.as_deref(), &req.base_url, &req.model, req.context_window, direct_key)?
        }
        "hermes" => hermes_apply(
            existing
                .as_deref()
                .ok_or("hermes config.yaml 不存在，無法切換（請先執行一次 hermes 完成初始化）")?,
            &req.base_url,
            &req.model,
        )?,
        _ => unreachable!(),
    };

    let mut result = SwitchResult {
        config_path: cfg.to_string_lossy().to_string(),
        backup_path: None,
        extra_files: vec![],
    };
    // 備份（同名 stem 前綴輪換；輪換時永遠保留乾淨備份——多次切換來源會
    // 堆積接管態備份，舊邏輯會把唯一的接管前備份淘汰掉導致無法還原）。
    if let Some(old) = existing.as_deref() {
        let dir = backups_root.join(&req.app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = cfg
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");
        if req.via_gateway
            && !has_baseline(&dir, stem)
            && !backup_is_tainted(&req.app, old, port)
        {
            write_baseline(&dir, stem, old).map_err(|e| e.to_string())?;
        }
        let name = format!("{stem}.bak-{}", crate::fsutil::backup_stamp());
        let dest = crate::fsutil::unique_backup_name(&dir, &name);
        std::fs::write(&dest, old).map_err(|e| format!("備份失敗：{e}"))?;
        prune_backups_keep_clean(&dir, &req.app, stem, BACKUP_KEEP, port)
            .map_err(|e| e.to_string())?;
        result.backup_path = Some(dest.to_string_lossy().to_string());
    }
    crate::fsutil::atomic_write(&cfg, new_text.as_bytes()).map_err(|e| format!("寫入失敗：{e}"))?;

    // Hermes 金鑰進自家 .env（行式追加/替換，同樣先備份）。
    if req.app == "hermes" {
        let env_path = hermes_home().join(".env");
        let old = read_text(&env_path).unwrap_or_default();
        if !old.is_empty() {
            let dir = backups_root.join("hermes");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let dest = crate::fsutil::unique_backup_name(
                &dir,
                &format!(".env.bak-{}", crate::fsutil::backup_stamp()),
            );
            std::fs::write(&dest, &old).map_err(|e| format!(".env 備份失敗：{e}"))?;
            crate::fsutil::rotate_backups(&dir, ".env", BACKUP_KEEP)
                .map_err(|e| e.to_string())?;
            result
                .backup_path
                .get_or_insert(dest.to_string_lossy().to_string());
        }
        let new_env = dotenv_set(&old, HERMES_CUSTOM_ENV_KEY, &req.api_key);
        crate::fsutil::atomic_write(&env_path, new_env.as_bytes())
            .map_err(|e| format!(".env 寫入失敗：{e}"))?;
        result.extra_files.push(env_path.to_string_lossy().to_string());
    }
    Ok(result)
}

/// Codex 接管前體檢：返回逐行結論（✅ 正常 / ⚠️ 接管時處理 / ❌ 先手工排除）。
/// 純讀取，不寫任何檔案。
pub fn codex_doctor_at(codex_home: &Path, port: u16) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let gw = format!("http://{GATEWAY_HOST}:{port}/v1");
    let cfg_path = codex_home.join("config.toml");
    match std::fs::read_to_string(&cfg_path) {
        Err(_) => out.push("⚠️ config.toml 不存在：接管將新建".to_string()),
        Ok(t) => {
            for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                let header = format!("[model_providers.{id}]");
                let n = t.lines().filter(|l| l.trim() == header).count();
                if n > 1 {
                    out.push(format!(
                        "❌ {header} 重複 {n} 次（非法 TOML）：接管入口將自動去重消毒"
                    ));
                }
            }
            match dedupe_codex_sections(&t).parse::<toml_edit::DocumentMut>() {
                Err(e) => out.push(format!("❌ config.toml 解析失敗：{e}")),
                Ok(doc) => {
                    if doc.get("model_provider").and_then(|v| v.as_str())
                        != Some(CODEX_SHARED_PROVIDER_ID)
                    {
                        out.push("⚠️ model_provider 不是 custom：接管將切到共享段名".to_string());
                    }
                    for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                        match doc
                            .get("model_providers")
                            .and_then(|m| m.get(id))
                            .and_then(|s| s.get("base_url"))
                            .and_then(|u| u.as_str())
                        {
                            Some(u) if u == gw => {}
                            Some(u) => out.push(format!(
                                "⚠️ [model_providers.{id}] base_url = {u}，不是本網關 {gw}（接管時將覆寫為網關；僅提示，不影響開關按鈕）"
                            )),
                            None => {
                                out.push(format!("⚠️ [model_providers.{id}] 缺失：接管將補建"))
                            }
                        }
                    }
                    match doc.get("model_catalog_json").and_then(|v| v.as_str()) {
                        Some(c) if !std::path::Path::new(c).exists() => {
                            out.push(format!("⚠️ model_catalog_json 指向缺失：{c}（接管將重寫）"))
                        }
                        None => out.push("⚠️ 未設 model_catalog_json：接管將按來源生成".to_string()),
                        _ => {}
                    }
                }
            }
        }
    }
    // **這裡以前會把讀取失敗報成 ✅。** 讀不到時 `codex_legacy_providers()`
    // 回傳空 vec，於是體檢顯示「歷史會話無第三方 provider 殘留」——一個失敗
    // 被當成通過，使用者唯一的診斷工具反而誤導他。現在失敗一律 ❌ 並附原因。
    match codex_legacy_providers_report(codex_home) {
        LegacyProviders::Failed { reason } => out.push(format!(
            "❌ 讀不到 Codex 歷史會話的 provider 名：{reason}。接管將無法補寫別名段，\
             仍在使用舊會話的 provider 可能失效 —— 請完全結束 Codex 後重試"
        )),
        LegacyProviders::Ok { providers, db } => {
            let src = db.file_name().unwrap_or_default().to_string_lossy().to_string();
            if providers.is_empty() {
                out.push(format!(
                    "✅ 歷史會話無第三方 provider 殘留（custom / tokengateway 之外；讀自 {src}）"
                ));
            } else {
                out.push(format!(
                    "ℹ️ 歷史會話用過 {} 個別名 provider（讀自 {src}）：{} —— 接管將全寫為網關別名段",
                    providers.len(),
                    providers.join(", ")
                ));
            }
        }
    }
    if std::env::var(GATEWAY_ENV_KEY)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        out.push(format!("✅ {GATEWAY_ENV_KEY} 已在環境變數"));
    } else {
        out.push(format!(
            "❌ {GATEWAY_ENV_KEY} 未設定：Codex 行程繼承不到 Key 會 401，請先設為用戶環境變數"
        ));
    }
    // cc-switch 代理殘留（15721）會與接管預期衝突
    let probe: std::net::SocketAddr = format!("{GATEWAY_HOST}:15721").parse().unwrap();
    if std::net::TcpStream::connect_timeout(&probe, std::time::Duration::from_millis(300)).is_ok()
    {
        out.push("❌ 15721 被佔用（疑似 cc-switch 代理仍在跑）：先完全退出 cc-switch 再接管".to_string());
    } else {
        out.push("✅ 15721 空閒（cc-switch 代理未運行）".to_string());
    }
    out
}

/// Codex 接管前體檢（用戶主目錄版）。
pub fn codex_doctor(port: u16) -> Vec<String> {
    match user_home() {
        Ok(h) => codex_doctor_at(&h.join(".codex"), port),
        Err(e) => vec![format!("❌ 用戶目錄不可用：{e}")],
    }
}

/// 備份是否為本工具的網關接管產物（還原時應跳過）。
fn backup_is_tainted(app: &str, text: &str, port: u16) -> bool {
    match app {
        "codex" => codex_text_base_url(text)
            .or_else(|| {
                text.parse::<toml_edit::DocumentMut>().ok().and_then(|doc| {
                    ["custom", GATEWAY_PROVIDER_ID]
                        .into_iter()
                        .find_map(|id| {
                            doc.get("model_providers")
                                .and_then(|p| p.get(id))
                                .and_then(|p| p.get("base_url"))
                                .and_then(|u| u.as_str())
                                .map(|u| u.to_string())
                        })
                })
            })
            .is_some_and(|u| is_gateway(&u, port)),
        "claude" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("env")
                    .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                    .and_then(|u| u.as_str())
                    .map(|u| is_gateway(u, port))
            })
            .unwrap_or(false),
        "opencode" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("provider")
                    .and_then(|p| p.get(GATEWAY_PROVIDER_ID))
                    .and_then(|p| p.get("options"))
                    .and_then(|o| o.get("baseURL"))
                    .and_then(|u| u.as_str())
                    .map(|u| is_gateway(u, port))
            })
            .unwrap_or(false),
        "hermes" => {
            let (provider, _) = parse_hermes_model(text);
            provider
                .as_deref()
                .and_then(|name| hermes_provider_base(text, name))
                .is_some_and(|u| is_gateway(&u, port))
                || text.contains(&format!("  {GATEWAY_PROVIDER_ID}:\n"))
        }
        _ => false,
    }
}

/// 備份輪換（保乾淨版）：總數超限時優先淘汰最舊的接管態備份，
/// 乾淨（接管前）備份永遠保留——它是關閉接管的唯一退路。
/// 全是乾淨備份卻超限時才退化為淘汰最舊者（與舊邏輯一致）。
fn prune_backups_keep_clean(
    dir: &Path,
    app: &str,
    stem: &str,
    keep: usize,
    port: u16,
) -> std::io::Result<()> {
    let prefix = format!("{stem}.bak-");
    let mut olds: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    olds.sort();
    while olds.len() > keep {
        let victim = olds
            .iter()
            .position(|p| {
                std::fs::read_to_string(p)
                    .map(|t| backup_is_tainted(app, &t, port))
                    .unwrap_or(false)
            })
            .unwrap_or(0);
        let v = olds.remove(victim);
        let _ = std::fs::remove_file(&v);
    }
    Ok(())
}

fn backup_has_known_route(app: &str, text: &str) -> bool {
    match app {
        "codex" => codex_text_base_url(text).is_some(),
        "claude" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("env")
                    .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                    .and_then(|u| u.as_str())
                    .map(|_| true)
            })
            .unwrap_or(false),
        "opencode" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("provider")
                    .and_then(|p| p.get(GATEWAY_PROVIDER_ID))
                    .and_then(|p| p.get("options"))
                    .and_then(|o| o.get("baseURL"))
                    .and_then(|u| u.as_str())
                    .map(|_| true)
            })
            .unwrap_or(false),
        "hermes" => {
            let (provider, _) = parse_hermes_model(text);
            provider
                .as_deref()
                .and_then(|name| hermes_provider_base(text, name))
                .is_some()
        }
        _ => false,
    }
}

fn backup_files(dir: &Path, stem: &str) -> std::io::Result<Vec<PathBuf>> {
    let prefix = format!("{stem}.bak-");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn has_baseline(dir: &Path, stem: &str) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.baseline-"))
            })
        })
        .unwrap_or(false)
}

fn remove_stale_baselines(dir: &Path, stem: &str) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for p in rd.filter_map(|e| e.ok()).map(|e| e.path()) {
            if p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{stem}.baseline-")))
            {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

fn write_baseline(dir: &Path, stem: &str, text: &str) -> std::io::Result<PathBuf> {
    remove_stale_baselines(dir, stem);
    let path = dir.join(format!(
        "{stem}.baseline-{}",
        crate::fsutil::backup_stamp()
    ));
    std::fs::write(&path, text)?;
    Ok(path)
}

fn migrate_gateway_baseline(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
) -> Result<(), String> {
    let stem = cfg
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let dir = app_data.join("backups").join(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if has_baseline(&dir, stem) {
        let mut existing = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.baseline-"))
            })
            .collect::<Vec<_>>();
            existing.sort_by_key(|e| e.file_name());
            if let Some(path) = existing.last() {
                let text = std::fs::read_to_string(path.path()).map_err(|e| e.to_string())?;
                if backup_has_known_route(app, &text) && !backup_is_tainted(app, &text, port) {
                    return Ok(());
                }
            }
    }
    remove_stale_baselines(&dir, stem);
    let current = std::fs::read_to_string(cfg).unwrap_or_default();
    if backup_has_known_route(app, &current) && !backup_is_tainted(app, &current, port) {
        write_baseline(&dir, stem, &current).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut candidates = backup_files(&dir, stem).map_err(|e| e.to_string())?;
    candidates.reverse();
    for path in candidates {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if backup_has_known_route(app, &text) && !backup_is_tainted(app, &text, port) {
            write_baseline(&dir, stem, &text).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("所有備份都還含網關配置，無法還原到接管前".into())
}

/// 一鍵還原：把接管前的備份寫回配置文件。
/// 從新到舊掃描 `{stem}.bak-*`，跳過仍含網關痕跡的版本（多次切換會產生多層備份）。
///
/// 註：原本另有 `restore_backup_to(...)`（無 port 參數）只被測試呼叫，且它傳
/// `migrate: false`，是「不做 baseline 遷移」的變體（B7）。為免生產 API 出現
/// 兩個語義不同的同名還原入口，已把它降為測試模組內的 helper；生產呼叫端一律
/// 走本函式（migrate=true）。
pub fn restore_backup_to_port(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
) -> Result<String, String> {
    restore_backup_to_inner(app_data, app, cfg, port, true)
}

fn restore_backup_to_inner(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
    migrate: bool,
) -> Result<String, String> {
    if !["claude", "codex", "opencode", "hermes"].contains(&app) {
        return Err("該工具不支援一鍵還原".into());
    }
    let stem = cfg
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    if migrate {
        if let Err(e) = migrate_gateway_baseline(app_data, app, cfg, port) {
            if has_baseline(
                &app_data.join("backups").join(app),
                cfg.file_name().and_then(|n| n.to_str()).unwrap_or("config"),
            ) {
                let _ = e;
            } else {
                return Err(e);
            }
        }
    }
    let dir = app_data.join("backups").join(app);
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|_| "找不到該工具的備份目錄，無法還原".to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| {
                    n.starts_with(&format!("{stem}.bak-"))
                        || n.starts_with(&format!("{stem}.baseline-"))
                })
        })
        .collect();
    if cands.is_empty() {
        return Err("找不到接管前的備份，無法還原".into());
    }
    cands.sort();
    cands.reverse();
    let mut skipped = 0usize;
    for c in &cands {
        let text = std::fs::read_to_string(c).map_err(|e| format!("備份讀取失敗：{e}"))?;
        // 跳過接管產物（provider 段名 / 網關本地端口）；用戶指向其他本機服務的原設定不在此列。
        if backup_is_tainted(app, &text, port) {
            skipped += 1;
            continue;
        }
        crate::fsutil::atomic_write(cfg, text.as_bytes())
            .map_err(|e| format!("還原寫入失敗：{e}"))?;
        // 透明還原：告知還原自哪個備份、跳過幾個、內容摘要；死鏈（直連 Chat 上游卻走 responses）明確警告。
        let name = c
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let summary = restore_summary(&text);
        let skip_note = if skipped > 0 {
            format!("（跳過 {skipped} 個含網關痕跡的備份）")
        } else {
            String::new()
        };
        // Hermes 連帶還原 .env（接管時寫入了我方 Key 行；只還主配置會殘留）。
        let env_note = if app == "hermes" {
            restore_hermes_env(app_data, cfg)?
        } else {
            String::new()
        };
        // 還原到 cc-switch 託管態：能用才算數（需啟動 cc-switch），明確告知。
        let switch_note = if text.contains("127.0.0.1:15721") || text.contains("localhost:15721") {
            "｜⚠️ 還原後走 cc-switch（15721），請先啟動 cc-switch，否則工具連不上"
        } else {
            ""
        };
        if migrate {
            remove_stale_baselines(&dir, stem);
        }
        return Ok(format!(
            "{}｜還原自 {name}{skip_note}{env_note}{switch_note}｜{summary}",
            cfg.to_string_lossy()
        ));
    }
    Err("所有備份都還含網關配置，無法還原到接管前".into())
}

/// Hermes `.env` 連帶還原：取最新不含我方 Key 行的 `.env.bak-*` 寫回；
/// 無任何 `.env` 備份時，若現文件僅有接管時新建的我方 Key 一行則刪除（還原"不存在"原狀）；
/// 否則保持現狀並注記（不猜、不刪用户內容）。
fn restore_hermes_env(app_data: &Path, cfg: &Path) -> Result<String, String> {
    let dir = app_data.join("backups").join("hermes");
    let home = cfg.parent().ok_or("hermes 配置目錄不可用".to_string())?;
    let envp = home.join(".env");
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(".env.bak-"))
                })
                .collect()
        })
        .unwrap_or_default();
    cands.sort();
    cands.reverse();
    for c in &cands {
        let text = std::fs::read_to_string(c).map_err(|e| format!(".env 備份讀取失敗：{e}"))?;
        if text.contains(HERMES_CUSTOM_ENV_KEY) {
            continue;
        }
        crate::fsutil::atomic_write(&envp, text.as_bytes())
            .map_err(|e| format!(".env 還原寫入失敗：{e}"))?;
        let name = c
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        return Ok(format!("｜.env 已還原自 {name}"));
    }
    if cands.is_empty() {
        if let Ok(cur) = std::fs::read_to_string(&envp) {
            let meaningful: Vec<&str> = cur
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            if meaningful.len() == 1
                && meaningful[0].starts_with(&format!("{HERMES_CUSTOM_ENV_KEY}="))
            {
                std::fs::remove_file(&envp).map_err(|e| format!(".env 移除失敗：{e}"))?;
                return Ok("｜.env 為接管時新建，已移除".to_string());
            }
        }
        return Ok("｜.env 無備份，保持現狀".to_string());
    }
    Ok("｜.env 無乾淨備份，保持現狀".to_string())
}

/// 還原內容摘要（行級掃描，不依賴嚴格解析）＋死鏈警告。
fn restore_summary(text: &str) -> String {
    let mut mp = "未知".to_string();
    let mut model = "未知".to_string();
    for line in text.lines() {
        let t = line.trim();
        if mp == "未知" && t.starts_with("model_provider") {
            if let Some(v) = t.split('=').nth(1) {
                mp = v.trim().trim_matches(['"', '\'']).to_string();
            }
        }
        if model == "未知" && (t.starts_with("model ") || t.starts_with("model=") || t.starts_with("model\t")) {
            if let Some(v) = t.split('=').nth(1) {
                model = v.trim().trim_matches(['"', '\'']).to_string();
            }
        }
        if mp != "未知" && model != "未知" {
            break;
        }
    }
    let mut out = format!("provider={mp} model={model}");
    if text.parse::<toml_edit::DocumentMut>().is_err() {
        out.push_str("｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入");
    }
    // 直連第三方 https 但 wire_api=responses：多數第三方只說 Chat 協議，Codex 將 404。
    let direct_chat = text.contains("https://")
        && text.contains("wire_api = \"responses\"")
        && !text.contains("api.openai.com")
        && !text.contains(GATEWAY_HOST);
    if direct_chat {
        out.push_str("｜⚠️ 該配置直連第三方 URL 但走 responses 協議（直連 Chat 上游會 404），能用才算數；不可用請改走網關接管或官方登入");
    }
    out
}

/// 還原指定工具到接管前狀態（埠由呼叫端提供）。
/// 註：原本另有 `restore_backup(app_data, app)` 預設埠包裝，只被測試呼叫而成為
/// dead_code（B7），已降為測試模組內的 helper。
pub fn restore_backup_port(app_data: &Path, app: &str, port: u16) -> Result<String, String> {
    let cfg = config_path_for(app)?;
    restore_backup_to_port(app_data, app, &cfg, port)
}

// ---------------------------------------------------------------- 單測 ---

#[cfg(test)]
mod tests {
    use super::*;

    /// 測試便利包裝：以預設埠呼叫正式還原路徑（migrate=true）。
    ///
    /// 這兩個函式原本是生產碼（`restore_backup_to` / `restore_backup`）。
    /// 它們只被測試呼叫，因此在 lib 建置下恆為 dead_code（B7）。
    /// `restore_backup` 是 `restore_backup_port` 的預設埠等價包裝，直接對應；
    /// `restore_backup_to` 則不同 —— 它傳 `migrate: false`，
    /// 是「不做 baseline 遷移」的還原路徑，測試需要精確覆蓋它，
    /// 故這裡照抄原行為而非改走 `restore_backup_to_port`。
    fn restore_backup_to(app_data: &Path, app: &str, cfg: &Path) -> Result<String, String> {
        restore_backup_to_inner(app_data, app, cfg, crate::db::DEFAULT_GATEWAY_PORT, false)
    }

    fn restore_backup(app_data: &Path, app: &str) -> Result<String, String> {
        restore_backup_port(app_data, app, crate::db::DEFAULT_GATEWAY_PORT)
    }

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
    fn restore_uses_effective_codex_provider_url() {
        let direct = "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
        let managed = "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\n[model_providers.tokengateway]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
        assert!(!backup_is_tainted("codex", direct, 15722));
        assert!(backup_is_tainted("codex", managed, 15722));
    }

    #[test]
    fn restore_skips_gateway_backups() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "current").unwrap();
        let bdir = app_data.join("backups").join("codex");
        std::fs::create_dir_all(&bdir).unwrap();
        std::fs::write(
            bdir.join("config.toml.bak-20260101-000000"),
            "model = \"orig\"\n",
        )
        .unwrap();
        std::fs::write(
            bdir.join("config.toml.bak-20260201-000000"),
            "model_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        )
        .unwrap();
        let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
        // 透明還原：返回值含配置路徑＋備份名＋摘要
        assert!(back.contains(&cfg.to_string_lossy().to_string()), "{back}");
        assert!(back.contains("還原自 config.toml.bak-20260101-000000"), "{back}");
        assert!(back.contains("model=orig"), "{back}");
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            "model = \"orig\"\n"
        );
        assert!(restore_backup_to(app_data, "cursor", &cfg).is_err());
    }

    #[test]
    fn restore_allows_leftover_section_with_direct_url() {
        // 2026-09-24 codex 實例：接管殘留 tokengateway 段名、但 base_url 已改回
        // 直連上游——這是真正的接管前配置，必須能還原（舊段名規則會誤判卡死）。
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "current").unwrap();
        let bdir = app_data.join("backups").join("codex");
        std::fs::create_dir_all(&bdir).unwrap();
        let direct = "model = \"moonshotai/kimi-k3\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n\n[model_providers.tokengateway]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n";
        std::fs::write(bdir.join("config.toml.bak-20260101-000000"), direct).unwrap();
        std::fs::write(
            bdir.join("config.toml.bak-20260201-000000"),
            "model_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        )
        .unwrap();
        let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
        assert!(back.contains("config.toml.bak-20260101-000000"), "{back}");
        assert!(back.contains("跳過 1 個含網關痕跡的備份"), "{back}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), direct);
    }

    #[test]
    fn restore_ccswitch_state_warns_not_blocks() {
        // 接管前在用 cc-switch：允許還原（忠於接管前），但必須警告需啟動 cc-switch。
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "current").unwrap();
        let bdir = app_data.join("backups").join("codex");
        std::fs::create_dir_all(&bdir).unwrap();
        std::fs::write(
            bdir.join("config.toml.bak-20260101-000000"),
            "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15721/v1\"\n",
        )
        .unwrap();
        let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
        assert!(back.contains("config.toml.bak-20260101-000000"), "{back}");
        assert!(back.contains("cc-switch"), "{back}");
    }

    #[test]
    fn restore_migrates_clean_backup_to_immutable_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.toml");
        std::fs::write(
            &cfg,
            "model = \"m\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        )
        .unwrap();
        let bdir = app_data.join("backups").join("codex");
        std::fs::create_dir_all(&bdir).unwrap();
        let direct = "model = \"m\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
        std::fs::write(bdir.join("config.toml.bak-20260101-000000"), direct).unwrap();
        let back = restore_backup_to_port(app_data, "codex", &cfg, 15722).unwrap();
        assert!(back.contains("baseline-"), "{back}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), direct);
        assert!(!has_baseline(&bdir, "config.toml"));
    }

    #[test]
    fn prune_keeps_clean_backups() {
        // 10 個接管態＋1 個最舊的乾淨備份：輪換必須淘汰接管態，乾淨備份存活。
        let dir = tempfile::tempdir().unwrap();
        let bdir = dir.path();
        std::fs::write(
            bdir.join("config.toml.bak-20260101-000000"),
            "model = \"orig\"\n",
        )
        .unwrap();
        for i in 2..=11 {
            std::fs::write(
                bdir.join(format!("config.toml.bak-202602{i:02}-000000")),
                "model_provider = \"tokengateway\"\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
            )
            .unwrap();
        }
        prune_backups_keep_clean(bdir, "codex", "config.toml", 10, 15722).unwrap();
        let left: Vec<String> = std::fs::read_dir(bdir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left.len(), 10, "{left:?}");
        assert!(left.contains(&"config.toml.bak-20260101-000000".to_string()), "{left:?}");
    }

    #[test]
    fn restore_warns_direct_chat_upstream() {
        // 直連 Chat 上游卻走 responses 的備份：照樣還原，但必須警告 404 風險
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.toml");
        std::fs::write(&cfg, "current").unwrap();
        let bdir = app_data.join("backups").join("codex");
        std::fs::create_dir_all(&bdir).unwrap();
        std::fs::write(
            bdir.join("config.toml.bak-20260301-000000"),
            "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n",
        )
        .unwrap();
        let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
        assert!(back.contains("config.toml.bak-20260301-000000"), "{back}");
        assert!(back.contains("provider=custom"), "{back}");
        assert!(back.contains("404"), "{back}");
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n"
        );
    }

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

    /// 真實接管（默认忽略，需显式 `-- --ignored live_codex_takeover` 執行）：
    /// 對真實 ~/.codex/config.toml 做一次網關接管（默認 NIM 渠道 15）。
    /// 按需改 LIVE_* 常量：LIVE_PID（來源 id；16 等已刪 id 用於驗證守衛攔截）、
    /// LIVE_MODEL、LIVE_DIRECT（true = 直連上游 Key，不經本地 Key）。
    /// 前置：Codex 完全退出；TOKEN_GATEWAY_KEY 已設為用戶環境變量。
    const LIVE_PID: i64 = 15;
    const LIVE_MODEL: &str = "deepseek-ai/deepseek-v4-flash-0731";
    const LIVE_DIRECT: bool = true;
    #[test]
    #[ignore = "live: writes real ~/.codex/config.toml"]
    fn live_codex_takeover() {
        // 與命令層 switch_apply 相同的來源有效性守衛（已刪/停用來源拒絕寫入）。
        let home = dirs::home_dir().expect("home");
        let app_data = home
            .join("AppData")
            .join("Roaming")
            .join("com.tokencounter.gateway");
        let conn =
            rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
        let alive: bool = conn
            .query_row(
                "SELECT enabled FROM providers WHERE id=?1",
                rusqlite::params![LIVE_PID],
                |r| r.get::<_, i64>(0),
            )
            .map(|e| e == 1)
            .unwrap_or(false);
        assert!(
            alive,
            "守衛生效：來源 {LIVE_PID} 不存在或已停用，拒絕接管（config 未被改動）"
        );
        let catalog =
            crate::models::codex_catalog_generate(&conn, &home, &app_data, LIVE_PID, false)
                .expect("catalog");
        // 直連模式：注入該來源上游 Key（復刻命令層注入）。
        let api_key = if LIVE_DIRECT {
            conn.query_row(
                "SELECT api_key FROM providers WHERE id=?1",
                rusqlite::params![LIVE_PID],
                |r| r.get::<_, String>(0),
            )
            .expect("upstream key")
        } else {
            String::new()
        };
        drop(conn);
        let req = SwitchRequest {
            app: "codex".into(),
            base_url: "http://127.0.0.1:15722/v1".into(),
            api_key,
            model: LIVE_MODEL.into(),
            via_gateway: true,
            provider_id: Some(LIVE_PID),
            provider_format: None,
            reasoning: Some("medium".into()),
            context_window: None,
            gen_catalog: true,
            catalog_union: false,
            direct_upstream: LIVE_DIRECT,
            key_id: None,
            claude_map: None,
        };
        let res = apply_switch(
            &app_data,
            req,
            15722,
            Some(catalog.to_string_lossy().to_string()),
        )
        .expect("apply");
        println!("takeover ok: {res:?}");
    }

    // ============ 開關往返 live 測試（真機，需顯式單線程執行） ============
    // 跑法：cargo test live_ -- --ignored --test-threads=1
    // 約定：每個測試先快照→動作→採集結果→teardown（寫回快照、刪測試期新備份）→再斷言，
    // 因此即使斷言變紅，機器現場也已恢復（另有 Temp 手工快照兜底）。

    fn live_dirs() -> (std::path::PathBuf, std::path::PathBuf) {
        let home = dirs::home_dir().expect("home");
        let app_data = home
            .join("AppData")
            .join("Roaming")
            .join("com.tokencounter.gateway");
        (home, app_data)
    }

    /// 復刻命令層直連注入：只讀 DB 取來源上游 Key。
    fn live_upstream(app_data: &std::path::Path, pid: i64) -> String {
        let conn =
            rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
        let up: String = conn
            .query_row(
                "SELECT api_key FROM providers WHERE id=?1 AND enabled=1",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .expect("upstream key");
        assert!(!up.trim().is_empty(), "來源 {pid} 未填寫上游 Key");
        up
    }

    /// 復刻前端開關 ON 的請求（經網關；claude/hermes 填上游 Key 明文，
    /// opencode 空 Key 走 direct_upstream 由後端注入）。
    fn live_req(app: &str, base_url: &str, key: String, direct: bool) -> SwitchRequest {
        SwitchRequest {
            app: app.into(),
            base_url: base_url.into(),
            api_key: key,
            model: LIVE_MODEL.into(),
            via_gateway: true,
            provider_id: Some(LIVE_PID),
            provider_format: None,
            reasoning: None,
            context_window: None,
            gen_catalog: false,
            catalog_union: false,
            direct_upstream: direct,
            key_id: None,
            claude_map: None,
        }
    }

    fn live_bak_names(app_data: &std::path::Path, app: &str) -> Vec<String> {
        let dir = app_data.join("backups").join(app);
        let mut v: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// 刪測試期新增的備份（以前後名單差集判定），讓備份目錄回到測試前。
    fn live_rm_new_baks(app_data: &std::path::Path, app: &str, before: &[String]) {
        let dir = app_data.join("backups").join(app);
        for n in live_bak_names(app_data, app) {
            if !before.contains(&n) {
                let _ = std::fs::remove_file(dir.join(n));
            }
        }
    }

    #[test]
    #[ignore = "live: writes real ~/.claude/settings.json (auto-restored)"]
    fn live_claude_roundtrip() {
        let (_home, app_data) = live_dirs();
        let cfg = config_path_for("claude").expect("claude path");
        let snap = std::fs::read(&cfg).expect("read settings");
        assert!(
            !String::from_utf8_lossy(&snap).contains(GATEWAY_PROVIDER_ID)
                && !String::from_utf8_lossy(&snap).contains("127.0.0.1"),
            "前置：claude 當前應為乾淨原設定"
        );
        let before = live_bak_names(&app_data, "claude");
        let up = live_upstream(&app_data, LIVE_PID);
        // T1 ON
        let r1 = apply_switch(&app_data, live_req("claude", "http://127.0.0.1:15722", up.clone(), false), 15722, None).expect("on#1");
        println!("on#1: {r1:?}");
        let on1 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
        let on1_ok = on1.contains("127.0.0.1:15722") && on1.contains(&up) && serde_json::from_str::<serde_json::Value>(&on1).is_ok();
        // T2 ON→ON（秒級戳，需跨秒以得不同備份名）
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let r2 = apply_switch(&app_data, live_req("claude", "http://127.0.0.1:15722", up.clone(), false), 15722, None).expect("on#2");
        println!("on#2: {r2:?}");
        let on2 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
        let on2_ok = on2.contains("127.0.0.1:15722");
        // OFF（與開關同一恢復函數）
        let msg = restore_backup(&app_data, "claude").expect("restore");
        println!("off: {msg}");
        let off = std::fs::read(&cfg).expect("read");
        let active = detect_tools(15722).iter().find(|t| t.app == "claude").is_some_and(|t| t.gateway_active);
        // teardown → 斷言
        live_rm_new_baks(&app_data, "claude", &before);
        std::fs::write(&cfg, &snap).expect("teardown");
        assert!(on1_ok, "ON#1 未正確接管：{on1}");
        assert!(on2_ok, "ON#2 未正確接管：{on2}");
        assert!(!active, "OFF 後檢測仍顯示接管中");
        assert_eq!(off, snap, "T1/T2：OFF 未位元組還原原設定");
    }

    #[test]
    #[ignore = "live: writes real opencode.json (auto-restored)"]
    fn live_opencode_roundtrip() {
        // 本機現狀：30 個託管備份全部含網關痕跡（最早的也是接管態），無乾淨原設定。
        // 本測試記錄該卡住狀態（pre-restore 必錯），並驗證 ON 寫入正確、現場歸零。
        let (_home, app_data) = live_dirs();
        let cfg = config_path_for("opencode").expect("opencode path");
        let snap = std::fs::read(&cfg).expect("read opencode.json");
        let before = live_bak_names(&app_data, "opencode");
        let up = live_upstream(&app_data, LIVE_PID);
        // OFF（預期失敗：無乾淨備份可還原）
        let pre = restore_backup(&app_data, "opencode");
        println!("pre-off: {pre:?}");
        let stuck = pre.is_err();
        // ON（直連上游：api_key 為命令層注入的上游 Key，此處直接復刻注入後狀態）
        let r1 = apply_switch(&app_data, live_req("opencode", "http://127.0.0.1:15722/v1", up.clone(), true), 15722, None).expect("on#1");
        println!("on#1: {r1:?}");
        let on1 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
        let v1: serde_json::Value = serde_json::from_str(&on1).expect("json");
        let on1_ok = v1["provider"]["tokengateway"]["options"]["baseURL"] == "http://127.0.0.1:15722/v1"
            && v1["provider"]["tokengateway"]["options"]["apiKey"] == serde_json::Value::String(up.clone());
        // teardown → 斷言（寫回測試前快照，機器狀態歸零）
        live_rm_new_baks(&app_data, "opencode", &before);
        std::fs::write(&cfg, &snap).expect("teardown");
        assert!(stuck, "預期本機 opencode 已無乾淨備份（pre-restore 應報錯），實際卻還原成功");
        assert!(on1_ok, "ON#1 未正確接管：{on1}");
    }

    #[test]
    #[ignore = "live: writes real hermes config + .env (auto-restored)"]
    fn live_hermes_roundtrip() {
        let (_home, app_data) = live_dirs();
        let cfg = config_path_for("hermes").expect("hermes path");
        let envp = hermes_home().join(".env");
        let snap_cfg = std::fs::read(&cfg).expect("read config.yaml");
        let snap_env = std::fs::read(&envp).expect("read .env");
        let before = live_bak_names(&app_data, "hermes");
        // 用另一來源的 Key（17）確保 ON 真改動 .env，否則測試是空轉的
        //（當前 .env 已含 15 的 Key，同值寫入位元組不變）。
        let up = live_upstream(&app_data, 17);
        // T1 ON（hermes 取上游 Key 直寫自家 .env）
        let r1 = apply_switch(&app_data, live_req("hermes", "http://127.0.0.1:15722/v1", up.clone(), false), 15722, None).expect("on#1");
        println!("on#1: {r1:?}");
        let on_cfg = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
        let on_env = String::from_utf8(std::fs::read(&envp).expect("read")).expect("utf8");
        let on_ok = on_cfg.contains(GATEWAY_PROVIDER_ID)
            && on_env.contains(&format!("{HERMES_CUSTOM_ENV_KEY}={up}"));
        // T2 ON→ON
        std::thread::sleep(std::time::Duration::from_millis(1200));
        apply_switch(&app_data, live_req("hermes", "http://127.0.0.1:15722/v1", up.clone(), false), 15722, None).expect("on#2");
        // OFF
        let msg = restore_backup(&app_data, "hermes").expect("restore");
        println!("off: {msg}");
        let off_cfg = std::fs::read(&cfg).expect("read");
        let off_env = std::fs::read(&envp).expect("read");
        // teardown → 斷言
        live_rm_new_baks(&app_data, "hermes", &before);
        std::fs::write(&cfg, &snap_cfg).expect("teardown cfg");
        std::fs::write(&envp, &snap_env).expect("teardown env");
        assert!(on_ok, "ON#1 未正確接管");
        assert_eq!(off_cfg, snap_cfg, "T1/T2：config.yaml 未還原");
        // .env 判據（本機 .env 自上次乾淨備份後另有合法漂移，不與快照比字節）：
        // 1) 接管殘留 Key 行必須消失；2) 內容等於最新乾淨備份（獨立掃描，不調恢復函數）。
        let off_env_s = String::from_utf8(off_env.clone()).expect("utf8");
        assert!(
            !off_env_s.contains(HERMES_CUSTOM_ENV_KEY),
            "T1/T2：.env 殘留接管 Key 行"
        );
        let dir = app_data.join("backups").join("hermes");
        let mut names = live_bak_names(&app_data, "hermes");
        names.retain(|n| n.starts_with(".env.bak-"));
        names.reverse();
        let newest_clean = names
            .iter()
            .map(|n| std::fs::read(dir.join(n)).expect("read bak"))
            .find(|b| {
                let s = String::from_utf8_lossy(b);
                !s.contains(HERMES_CUSTOM_ENV_KEY)
            })
            .expect("a clean .env backup");
        assert_eq!(off_env, newest_clean, "T1/T2：.env 未還原到最新乾淨備份");
    }

    #[test]
    #[ignore = "live: temporarily relocates real backup dirs (auto-moved-back)"]
    fn live_restore_no_backup() {
        // T3：無可用備份時，OFF 必須明確報錯且不碰配置文件。
        let (_home, app_data) = live_dirs();
        for app in ["claude", "opencode", "hermes"] {
            let cfg = config_path_for(app).expect("cfg path");
            let snap = std::fs::read(&cfg).expect("read cfg");
            let dir = app_data.join("backups").join(app);
            let aside = app_data.join("backups").join(format!("{app}.t3aside"));
            let _ = std::fs::remove_dir_all(&aside);
            std::fs::rename(&dir, &aside).expect("move aside");
            let res = restore_backup(&app_data, app);
            let after = std::fs::read(&cfg).expect("read cfg");
            std::fs::rename(&aside, &dir).expect("move back");
            let listed = live_bak_names(&app_data, app);
            assert!(res.is_err(), "T3 {app}：無備份時應報錯，實際 Ok");
            assert_eq!(after, snap, "T3 {app}：無備份還原動了配置文件");
            assert!(!listed.is_empty(), "T3 {app}：備份目錄未移回");
            println!("T3 {app}: Err={}（文件未動，目錄已移回）", res.unwrap_err());
        }
    }

    #[test]
    fn backup_classifier_uses_configured_gateway_port() {
        let managed = "model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:18099/v1\"\n";
        assert!(!backup_is_tainted("codex", managed, 15722));
        assert!(backup_is_tainted("codex", managed, 18099));
    }

    #[test]
    fn rotate_trims_timestamped_names() {
        // 輪換必須認 `{stem}.bak-{stamp}`（曾因 ends_with(".bak") 永不命中而無限堆積）。
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=12 {
            std::fs::write(
                dir.path().join(format!("settings.json.bak-20260901-0000{i:02}")),
                "x",
            )
            .unwrap();
        }
        crate::fsutil::rotate_backups(dir.path(), "settings.json", 10).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        assert_eq!(names.len(), 10, "{names:?}");
        assert!(names.iter().all(|n| n.as_str() > "settings.json.bak-20260901-000002"), "{names:?}");
    }

    #[test]
    fn backup_name_unique_within_same_second() {
        // 同秒重複備份不得覆蓋（否則丟失真正的接管前原設定）。
        let dir = tempfile::tempdir().unwrap();
        let base = "settings.json.bak-20260922-040000";
        let p1 = crate::fsutil::unique_backup_name(dir.path(), base);
        std::fs::write(&p1, "first").unwrap();
        let p2 = crate::fsutil::unique_backup_name(dir.path(), base);
        assert_ne!(p1, p2);
        std::fs::write(&p2, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&p1).unwrap(), "first");
        // 後綴不破壞"最新優先"排序（仍排最前）。
        let mut v = vec![p1, p2];
        v.sort();
        v.reverse();
        assert!(v[0].to_string_lossy().ends_with("-2"));
    }

    #[test]
    fn hermes_restore_brings_env_back() {
        // Hermes OFF 必須連帶還原 .env（tempdir 沙盒：cfg 父目錄即 .env 歸屬）。
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.yaml");
        std::fs::write(&cfg, "current").unwrap();
        std::fs::write(&cfg.parent().unwrap().join(".env"), "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-now\n").unwrap();
        let bdir = app_data.join("backups").join("hermes");
        std::fs::create_dir_all(&bdir).unwrap();
        let clean_cfg = "model:\n  provider: flatkey\n";
        let clean_env = "# user env\nFLAT=1\n";
        std::fs::write(bdir.join("config.yaml.bak-20260101-000000"), clean_cfg).unwrap();
        std::fs::write(bdir.join(".env.bak-20260101-000000"), clean_env).unwrap();
        std::fs::write(
            bdir.join("config.yaml.bak-20260201-000000"),
            "providers:\n  tokengateway:\n    base_url: http://127.0.0.1:15722/v1\n",
        )
        .unwrap();
        std::fs::write(
            bdir.join(".env.bak-20260201-000000"),
            "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-then\n",
        )
        .unwrap();
        let msg = restore_backup_to(app_data, "hermes", &cfg).expect("restore");
        assert!(msg.contains(".env 已還原自 .env.bak-20260101-000000"), "{msg}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), clean_cfg);
        assert_eq!(
            std::fs::read_to_string(cfg.parent().unwrap().join(".env")).unwrap(),
            clean_env
        );
    }

    #[test]
    fn hermes_env_all_tainted_keeps_current() {
        // .env 無乾淨備份時：保持現狀並注記，不猜不刪。
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("config.yaml");
        std::fs::write(&cfg, "current").unwrap();
        let cur = "FLAT=1\nHERMES_CUSTOM_TOKENGATEWAY_API_KEY=up\n";
        std::fs::write(cfg.parent().unwrap().join(".env"), cur).unwrap();
        let bdir = app_data.join("backups").join("hermes");
        std::fs::create_dir_all(&bdir).unwrap();
        std::fs::write(bdir.join("config.yaml.bak-20260101-000000"), "model:\n  provider: flatkey\n").unwrap();
        std::fs::write(
            bdir.join(".env.bak-20260101-000000"),
            "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-old\n",
        )
        .unwrap();
        let msg = restore_backup_to(app_data, "hermes", &cfg).expect("restore");
        assert!(msg.contains(".env 無乾淨備份，保持現狀"), "{msg}");
        assert_eq!(
            std::fs::read_to_string(cfg.parent().unwrap().join(".env")).unwrap(),
            cur
        );
    }

    #[test]
    fn version_cmp_and_clean() {
        use std::cmp::Ordering;
        assert_eq!(clean_version("codex-cli 0.153.4"), Some("0.153.4".to_string()));
        assert_eq!(clean_version("2.1.263 (Claude Code)"), Some("2.1.263".to_string()));
        assert_eq!(
            clean_version("Hermes Agent v0.21.1 (2026.9.7) · upstream abc"),
            Some("0.21.1".to_string())
        );
        assert_eq!(clean_version("0.1.2-rc.1"), Some("0.1.2-rc.1".to_string()));
        assert_eq!(clean_version("no version here"), None);
        assert_eq!(cmp_version("1.2.10", "1.2.9"), Ordering::Greater);
        assert_eq!(cmp_version("2.1.263", "2.1.263"), Ordering::Equal);
        assert_eq!(cmp_version("0.1.2", "0.1.2-rc.1"), Ordering::Greater);
        assert_eq!(cmp_version("0.1.2-rc.1", "0.1.2"), Ordering::Less);
        assert_eq!(cmp_version("0.153.4", "0.153.10"), Ordering::Less);
    }

    #[test]
    fn npm_pkg_version_reads_package_json() {
        // scoped 包路徑 @a\b + version 字段解析。
        let dir = tempfile::tempdir().unwrap();
        let pkgdir = dir.path().join("@anthropic-ai").join("claude-code");
        std::fs::create_dir_all(&pkgdir).unwrap();
        std::fs::write(pkgdir.join("package.json"), r#"{"name":"x","version":"2.1.263"}"#).unwrap();
        assert_eq!(
            npm_pkg_version_at(dir.path(), "@anthropic-ai/claude-code"),
            Some("2.1.263".to_string())
        );
        assert_eq!(npm_pkg_version_at(dir.path(), "nope"), None);
    }

    #[test]
    #[cfg(windows)]
    fn run_timeout_kills_hang() {
        // 回顯成功。
        let ok = run_with_timeout(
            &["cmd".to_string(), "/C".to_string(), "echo".to_string(), "hi".to_string()],
            10_000,
        )
        .unwrap();
        assert!(ok.contains("hi"), "{ok}");
        // ping 5 秒，800ms 限：必須超時殺掉。
        let err = run_with_timeout(
            &["cmd".to_string(), "/C".to_string(), "ping".to_string(), "-n".to_string(), "6".to_string(), "127.0.0.1".to_string()],
            800,
        )
        .unwrap_err();
        assert!(err.contains("超時"), "{err}");
    }

    #[test]
    #[ignore = "live: queries npm registry (network)"]
    fn live_tool_latest_print() {
        for l in tool_latest() {
            println!("{} latest={:?} up_to_date={:?}", l.app, l.latest, l.up_to_date);
        }
    }

    #[test]
    #[ignore = "live: probes real installed tool versions (read-only)"]
    fn live_tool_versions_shape() {
        // 契約：5 工具各一條；已安裝的有更新命令（cursor/antigravity 不在列）。
        let vs = tool_versions();
        assert_eq!(vs.len(), 5);
        for v in &vs {
            println!("{} installed={} version={:?} update={:?}", v.app, v.installed, v.version, v.update_label);
            if v.installed {
                assert!(!v.update_argv.is_empty(), "{} 已安裝卻無更新命令", v.app);
            }
        }
    }

    #[test]
    fn restore_returns_numeric_localhost_original() {
        // T4（tempdir，零風險）：用戶原設定若指向數字 127.0.0.1 的本機服務
        //（如 http://127.0.0.1:11434），不得被當成網關殘留跳過。
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let cfg = dir.path().join("settings.json");
        std::fs::write(&cfg, "current").unwrap();
        let bdir = app_data.join("backups").join("claude");
        std::fs::create_dir_all(&bdir).unwrap();
        let orig = r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:11434/v1"}}"#;
        std::fs::write(bdir.join("settings.json.bak-20260101-000000"), orig).unwrap();
        std::fs::write(
            bdir.join("settings.json.bak-20260201-000000"),
            r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:15722", "ANTHROPIC_AUTH_TOKEN": "sk-local-x"}}"#,
        )
        .unwrap();
        let back = restore_backup_to(app_data, "claude", &cfg).expect("應還原數字 localhost 原設定");
        assert!(back.contains("settings.json.bak-20260101-000000"), "{back}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), orig);
    }

    /// 真實接管（默认忽略，需显式 `-- --ignored live_opencode_direct` 執行）：
    /// 對真實 ~/.config/opencode/opencode.json 做一次直連上游接管（默認 NIM 渠道 15），
    /// 驗證注入的上游 Key 明文落盤（回归：曾靜默寫 {env:} 引用導致斷連；另需前端保證
    /// 空 Key 時 via_gateway=true，否則 baseURL 會是第三方直連）。
    /// 前置：opencode 未運行；跑完後用 backups/opencode 下的新備份還原。
    #[test]
    #[ignore = "live: writes real ~/.config/opencode/opencode.json"]
    fn live_opencode_direct() {
        let home = dirs::home_dir().expect("home");
        let app_data = home
            .join("AppData")
            .join("Roaming")
            .join("com.tokencounter.gateway");
        let conn =
            rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
        let alive: bool = conn
            .query_row(
                "SELECT enabled FROM providers WHERE id=?1",
                rusqlite::params![LIVE_PID],
                |r| r.get::<_, i64>(0),
            )
            .map(|e| e == 1)
            .unwrap_or(false);
        assert!(alive, "守衛生效：來源 {LIVE_PID} 不存在或已停用，拒絕接管");
        // 直連模式：注入該來源上游 Key（復刻命令層注入）。
        let up: String = conn
            .query_row(
                "SELECT api_key FROM providers WHERE id=?1",
                rusqlite::params![LIVE_PID],
                |r| r.get(0),
            )
            .expect("upstream key");
        assert!(!up.trim().is_empty(), "來源 {LIVE_PID} 未填寫上游 Key");
        drop(conn);
        let req = SwitchRequest {
            app: "opencode".into(),
            base_url: "http://127.0.0.1:15722/v1".into(),
            api_key: up.clone(),
            model: LIVE_MODEL.into(),
            via_gateway: true,
            provider_id: Some(LIVE_PID),
            provider_format: None,
            reasoning: None,
            context_window: None,
            gen_catalog: false,
            catalog_union: false,
            direct_upstream: true,
            key_id: None,
            claude_map: None,
        };
        let res = apply_switch(&app_data, req, 15722, None).expect("apply");
        println!("takeover ok: {res:?}");
        let text = std::fs::read_to_string(
            home.join(".config").join("opencode").join("opencode.json"),
        )
        .expect("opencode.json");
        let v: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(
            v["provider"]["tokengateway"]["options"]["baseURL"],
            "http://127.0.0.1:15722/v1",
            "直連模式 baseURL 仍是網關：{text}"
        );
        assert_eq!(
            v["provider"]["tokengateway"]["options"]["apiKey"], up,
            "上游 Key 必須明文落盤（不得是 env 引用）：{text}"
        );
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
}
