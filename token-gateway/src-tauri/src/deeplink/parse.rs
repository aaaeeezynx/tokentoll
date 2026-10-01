//! Deep Link 解析（P4.1，對標 cc-switch 的 `ccswitch://` 協定）。
//!
//! ## 格式（照 cc-switch 手冊 §5.3）
//!
//! ```text
//! tokengateway://v1/import?resource=provider&…
//! ```
//!
//! | resource | 必要參數 | 選用 |
//! |---|---|---|
//! | provider | `app`, `name` | `endpoint`（可逗號分隔多個）、`apiKey`、`model`、`notes`、`enabled` |
//!
//! 我們的協定名是 `tokengateway://`（不是 `ccswitch://`）；參數名稱與語意照抄，
//! 讓既有的分享連結只要換 scheme 就能用。
//!
//! **只支援 `provider`**：`mcp` / `prompt` / `skill` 三種資源連同對應功能已於
//! 2026-10-02 移除，舊連結會拿到「不支援的 resource」而不是靜默失敗。
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
        // mcp / prompt / skill 的功能已移除（2026-10-02）；這裡刻意只回一句人話，
        // 不做相容層 —— 舊連結會明確失敗，而不是靜默忽略。
        other => Err(format!("不支援的 resource：{other}（目前只支援 provider）")),
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
    }
}

fn yes_no(b: bool) -> String {
    if b { "是".into() } else { "否".into() }
}

#[cfg(test)]
mod tests;
