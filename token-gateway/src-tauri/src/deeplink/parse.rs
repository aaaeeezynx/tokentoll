//! Deep Link 解析（P4.1，對標 cc-switch 的 `ccswitch://` 協定）。
//!
//! ## 格式（照 cc-switch 手冊 §5.3）
//!
//! ```text
//! tokengateway://v1/import?resource={provider|mcp|prompt|skill}&…
//! ```
//!
//! | resource | 必要參數 | 選用 |
//! |---|---|---|
//! | provider | `app`, `name` | `endpoint`（可逗號分隔多個）、`apiKey`、`model`、`notes`、`enabled` |
//! | prompt | `app`, `name`, `content`（Base64） | `description`、`enabled` |
//! | mcp | `apps`（逗號分隔）、`config`（Base64 的 `{"mcpServers":{…}}`） | `enabled` |
//! | skill | `repo`（`owner/name`） | `directory`、`branch` |
//!
//! 我們的協定名是 `tokengateway://`（不是 `ccswitch://`）；參數名稱與語意照抄，
//! 讓既有的分享連結只要換 scheme 就能用。
//!
//! ## 安全（cc-switch 也做同一套）
//!
//! - **只解析，不落地**：`parse` 回傳一份可顯示的預覽，真正的寫入在 `apply`。
//! - 必填欄位、app／協議白名單、Base64／JSON 格式都先驗過。
//! - 金鑰等敏感值在預覽裡**只顯示前後綴**。

use serde::Serialize;

use crate::tools::TAKEOVER_APPS;

/// 解析後的匯入請求。
#[derive(Debug, Clone, PartialEq)]
pub enum ImportRequest {
    Provider(ProviderImport),
    Prompt(PromptImport),
    Mcp(McpImport),
    Skill(SkillImport),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProviderImport {
    pub app: String,
    pub name: String,
    /// 多個網址用逗號分隔（與 cc-switch 一樣）
    pub endpoints: Vec<String>,
    pub api_key: String,
    pub model: String,
    pub notes: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PromptImport {
    pub app: String,
    pub name: String,
    pub content: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct McpImport {
    pub apps: Vec<String>,
    pub enabled: bool,
    /// (server id, 我們的正規化輸入)
    pub servers: Vec<(String, crate::mcp::McpInput)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkillImport {
    pub repo: String,
    pub directory: String,
    pub branch: String,
    /// 技能名稱（沒有就從 directory 推）
    pub name: String,
}

/// 給前端顯示的預覽（**不含完整金鑰**）。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DeeplinkPreview {
    pub kind: String,
    pub title: String,
    /// 逐欄位摘要（已遮罩敏感值）
    pub fields: Vec<(String, String)>,
    /// 要提醒使用者的事（例如「這個連結帶有 API Key」）
    pub warnings: Vec<String>,
    /// 這次會建立／影響什麼（人類可讀）
    pub effects: Vec<String>,
}

// ------------------------------------------------------------ 解析 ---

/// 從完整 URL 解析出匯入請求（失敗回人話）。
pub fn parse(url: &str) -> Result<ImportRequest, String> {
    let rest = url
        .strip_prefix("tokengateway://")
        .ok_or_else(|| "不是 tokengateway:// 連結".to_string())?;
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p.trim_end_matches('/'), q),
        None => (rest.trim_end_matches('/'), ""),
    };
    if path != "v1/import" && path != "import" {
        return Err(format!("不認得這個路徑：{path}（預期 v1/import）"));
    }
    let params = parse_query(query);
    let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let resource = get("resource").ok_or_else(|| "缺少 resource 參數".to_string())?;
    match resource.as_str() {
        "provider" => parse_provider(&get),
        "prompt" => parse_prompt(&get),
        "mcp" => parse_mcp(&get),
        "skill" => parse_skill(&get),
        other => Err(format!("不支援的 resource：{other}")),
    }
}

/// `?a=1&b=2` → `[(a,1),(b,2)]`（含 percent-decode 與 `+` → 空白）。
fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

/// percent-decode（`%XX` 與 `+`）。刻意自己寫：只為了這個用途不需要整個 url crate。
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Base64（含 URL-safe 與缺 padding 的容忍 —— 分享連結常常被截掉 `=`）。
pub(crate) fn b64_decode(s: &str) -> Result<String, String> {
    use base64::Engine;
    let cleaned: String = s.trim().replace('-', "+").replace('_', "/");
    let padded = match cleaned.len() % 4 {
        2 => format!("{cleaned}=="),
        3 => format!("{cleaned}="),
        _ => cleaned,
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .map_err(|e| format!("Base64 解碼失敗：{e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("不是有效的 UTF-8：{e}"))
}

/// app 參數的白名單（我們只支援能接管的三個）。
fn check_app(app: &str) -> Result<String, String> {
    let a = app.trim().to_ascii_lowercase();
    if TAKEOVER_APPS.contains(&a.as_str()) {
        Ok(a)
    } else {
        Err(format!(
            "不支援的工具「{app}」（我們只管理 {}）",
            TAKEOVER_APPS.join("／")
        ))
    }
}

fn parse_provider(get: &dyn Fn(&str) -> Option<String>) -> Result<ImportRequest, String> {
    let app = check_app(&get("app").ok_or("provider 缺少 app")?)?;
    let name = get("name").unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return Err("provider 缺少 name".to_string());
    }
    let endpoints: Vec<String> = get("endpoint")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for e in &endpoints {
        if !e.starts_with("http://") && !e.starts_with("https://") {
            return Err(format!("endpoint 必須是 http(s)：{e}"));
        }
    }
    Ok(ImportRequest::Provider(ProviderImport {
        app,
        name,
        endpoints,
        api_key: get("apiKey").unwrap_or_default(),
        model: get("model").unwrap_or_default(),
        notes: get("notes").unwrap_or_default(),
        enabled: get("enabled").map(|v| v != "false" && v != "0").unwrap_or(true),
    }))
}

fn parse_prompt(get: &dyn Fn(&str) -> Option<String>) -> Result<ImportRequest, String> {
    let app = check_app(&get("app").ok_or("prompt 缺少 app")?)?;
    let name = get("name").unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return Err("prompt 缺少 name".to_string());
    }
    let content = b64_decode(&get("content").ok_or("prompt 缺少 content（Base64）")?)?;
    Ok(ImportRequest::Prompt(PromptImport {
        app,
        name,
        content,
        description: get("description").unwrap_or_default(),
        enabled: get("enabled").map(|v| v != "false" && v != "0").unwrap_or(false),
    }))
}

fn parse_mcp(get: &dyn Fn(&str) -> Option<String>) -> Result<ImportRequest, String> {
    let apps: Vec<String> = get("apps")
        .ok_or("mcp 缺少 apps")?
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| check_app(&s))
        .collect::<Result<Vec<_>, _>>()?;
    if apps.is_empty() {
        return Err("mcp 的 apps 是空的".to_string());
    }
    let raw = b64_decode(&get("config").ok_or("mcp 缺少 config（Base64）")?)?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("config 不是合法 JSON：{e}"))?;
    let map = json
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .ok_or("config 裡沒有 mcpServers 物件")?;
    let mut servers = vec![];
    for (id, v) in map {
        let input = super::super::mcp::input_from_json(id, v)
            .map_err(|e| format!("伺服器「{id}」不合法：{e}"))?;
        servers.push((id.clone(), input));
    }
    if servers.is_empty() {
        return Err("config 裡沒有任何伺服器".to_string());
    }
    Ok(ImportRequest::Mcp(McpImport {
        apps,
        enabled: get("enabled").map(|v| v != "false" && v != "0").unwrap_or(true),
        servers,
    }))
}

fn parse_skill(get: &dyn Fn(&str) -> Option<String>) -> Result<ImportRequest, String> {
    let repo = get("repo").ok_or("skill 缺少 repo（owner/name）")?;
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| format!("repo 格式應為 owner/name：{repo}"))?;
    let (owner, name) = crate::skills::validate_repo(owner, name, "main", "")
        .map(|(o, n, _, _)| (o, n))?;
    let directory = get("directory").unwrap_or_default();
    let fallback = directory
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let name_param = get("name").unwrap_or_default();
    Ok(ImportRequest::Skill(SkillImport {
        repo: format!("{owner}/{name}"),
        directory,
        branch: get("branch").unwrap_or_else(|| "main".into()),
        name: if name_param.trim().is_empty() {
            fallback
        } else {
            name_param.trim().to_string()
        },
    }))
}

// ------------------------------------------------------------ 預覽 ---

/// 金鑰遮罩：只留前 6 與後 4。
pub(crate) fn mask(secret: &str) -> String {
    let s = secret.trim();
    if s.is_empty() {
        return "（空）".to_string();
    }
    let n = s.chars().count();
    if n <= 12 {
        return "•".repeat(n.max(4));
    }
    let head: String = s.chars().take(6).collect();
    let tail: String = s.chars().skip(n - 4).collect();
    format!("{head}…{tail}")
}

/// 產生預覽（**只讀**；真正的寫入在 `apply`）。
pub fn preview(req: &ImportRequest) -> DeeplinkPreview {
    match req {
        ImportRequest::Provider(p) => {
            let mut warnings = vec![];
            if !p.api_key.trim().is_empty() {
                warnings.push("這個連結帶有 API Key，請只匯入你信任的來源".to_string());
            }
            if p.endpoints.is_empty() {
                warnings.push("連結沒有帶上游網址（endpoint），匯入後要自己填".to_string());
            }
            let mut fields = vec![
                ("工具".into(), p.app.clone()),
                ("名稱".into(), p.name.clone()),
                ("上游網址".into(), if p.endpoints.is_empty() {
                    "（未提供）".into()
                } else {
                    p.endpoints.join("、")
                }),
                ("API Key".into(), mask(&p.api_key)),
                ("預設模型".into(), if p.model.is_empty() { "（未提供）".into() } else { p.model.clone() }),
                ("啟用".into(), yes_no(p.enabled)),
            ];
            if !p.notes.is_empty() {
                fields.push(("備註".into(), p.notes.clone()));
            }
            DeeplinkPreview {
                kind: "provider".into(),
                title: format!("匯入來源「{}」", p.name),
                fields,
                warnings,
                effects: vec![
                    format!("在「上游來源」新增一個 {}", p.app),
                    if p.model.is_empty() {
                        "不會新增模型（連結沒帶 model）".into()
                    } else {
                        format!("同時登記模型「{}」", p.model)
                    },
                ],
            }
        }
        ImportRequest::Prompt(p) => DeeplinkPreview {
            kind: "prompt".into(),
            title: format!("匯入提示詞「{}」", p.name),
            fields: vec![
                ("工具".into(), p.app.clone()),
                ("名稱".into(), p.name.clone()),
                ("內容".into(), format!("{} 字元", p.content.chars().count())),
                ("內容預覽".into(), first_lines(&p.content, 3)),
                ("啟用".into(), yes_no(p.enabled)),
            ],
            warnings: if p.enabled {
                vec!["這個連結要求直接啟用 —— 啟用會**覆寫**該工具的提示詞檔".to_string()]
            } else {
                vec![]
            },
            effects: vec![
                "在「提示詞」新增一個預設集（預設不啟用）".into(),
                "啟用要你自己在提示詞頁按（會先備份現有檔案）".into(),
            ],
        },
        ImportRequest::Mcp(m) => {
            let ids: Vec<String> = m.servers.iter().map(|(id, _)| id.clone()).collect();
            let mut fields = vec![
                ("同步到".into(), m.apps.join("、")),
                ("伺服器".into(), ids.join("、")),
                ("啟用".into(), yes_no(m.enabled)),
            ];
            for (id, input) in &m.servers {
                let detail = if input.transport == "stdio" {
                    format!("{} {}", input.command, input.args.join(" "))
                } else {
                    format!("{} {}", input.transport, input.url)
                };
                fields.push((format!("· {id}"), detail.trim().to_string()));
            }
            DeeplinkPreview {
                kind: "mcp".into(),
                title: format!("匯入 {} 個 MCP 伺服器", m.servers.len()),
                fields,
                warnings: vec![
                    "MCP 伺服器會在你的電腦上執行指令 —— 只匯入你信任的來源".to_string(),
                ],
                effects: vec![
                    format!("在「MCP」新增 {} 個伺服器", m.servers.len()),
                    format!("並同步到 {}", m.apps.join("、")),
                ],
            }
        }
        ImportRequest::Skill(s) => DeeplinkPreview {
            kind: "skill".into(),
            title: format!("匯入技能「{}」", s.name),
            fields: vec![
                ("儲存庫".into(), s.repo.clone()),
                ("分支".into(), s.branch.clone()),
                ("目錄".into(), if s.directory.is_empty() { "（根目錄）".into() } else { s.directory.clone() }),
            ],
            warnings: vec!["會從 GitHub 下載這個技能（需要網路）".to_string()],
            effects: vec![
                "在「技能」新增一個來源儲存庫並安裝這個技能".into(),
                "安裝後可逐工具選擇要同步到哪些工具".into(),
            ],
        },
    }
}

fn yes_no(b: bool) -> String {
    if b { "是".into() } else { "否".into() }
}

fn first_lines(s: &str, n: usize) -> String {
    let joined = s
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .collect::<Vec<_>>()
        .join(" / ");
    crate::usage_query::truncate(&joined, 160)
}

#[cfg(test)]
mod tests;
