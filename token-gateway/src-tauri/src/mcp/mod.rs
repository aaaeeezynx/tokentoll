//! MCP 伺服器管理（P3.1，對標 cc-switch 的 MCP Management）。
//!
//! ## cc-switch 的模型（照它手冊 §3.1 抄）
//!
//! - **一個統一的面板**，七個工具共用；每個伺服器可以**逐工具**開啟／關閉。
//! - 新增：預設樣板（fetch／time／memory／sequential-thinking／context7）或自訂；
//!   欄位＝Server ID（必填、唯一）、名稱、說明、傳輸型別（stdio／http／sse）、
//!   Command（stdio 必填）、Arguments、URL（http／sse 必填）、Headers、環境變數。
//! - 開啟某個工具＝**寫進那個工具的設定檔**（Claude `~/.claude.json` 的 `mcpServers`、
//!   Codex `~/.codex/config.toml` 的 `[mcp_servers]`、OpenCode `opencode.json` 的 `mcp`）；
//!   關閉＝從那個檔案移除。**只動資料庫管得到的伺服器**，手寫的不碰。
//! - 同步條件：該工具有安裝才寫（沒有就靜默不寫、不報錯）。
//! - 匯入既有設定：一次讀遍各工具的設定檔，把伺服器收進來，並自動對來源工具啟用。
//!
//! ## 我們的範圍
//!
//! 使用者已裁示**不新增受管工具**，所以 MCP 只對本 App 能接管的三個工具同步：
//! **Claude Code／Codex／OpenCode**（與 `tools::TAKEOVER_APPS` 同一份清單 ——
//! 能寫設定檔的工具才可能同步 MCP，兩份清單不該各自維護）。
//!
//! 這也讓「寫入」沿用既有的安全機制：原子寫入 ＋ 改寫前備份（`fsutil`／`tools::backup`）。

mod apply;
mod presets;
mod store;
mod sync;

pub use presets::{presets, McpPreset};
pub use store::{
    delete as delete_server, list as list_servers, set_binding, upsert as upsert_server,
};
pub use sync::{import_existing, sync_all, sync_error, ImportReport, SyncOutcome};

use serde::{Deserialize, Serialize};

/// 一個 MCP 伺服器。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpServer {
    pub id: i64,
    /// Server ID（設定檔裡的鍵；唯一）
    pub slug: String,
    pub name: String,
    pub description: String,
    /// `stdio`／`http`／`sse`
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    pub headers: Vec<HeaderPair>,
    pub env: Vec<HeaderPair>,
    /// 逐工具啟用狀態（claude／codex／opencode）
    pub bindings: Vec<Binding>,
}

/// 一個伺服器對一個工具的啟用狀態。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Binding {
    pub app: String,
    pub enabled: bool,
}

/// 名稱／值配對（標頭與環境變數共用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

/// 新增／編輯的輸入。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpInput {
    /// 編輯時帶 id；新增時為 None
    pub id: Option<i64>,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    pub headers: Vec<HeaderPair>,
    pub env: Vec<HeaderPair>,
}

/// Server ID 的驗證：設定檔的鍵，必須是安全、好辨識的字串。
///
/// 允許英數與 `-`／`_`／`.`（很多伺服器叫 `mcp-server-fetch`、
/// `@modelcontextprotocol/server-memory` 之類），但不允許空白與引號 ——
/// 那些在 TOML 段名與 JSON 鍵裡都會變成人間凶器。
pub(crate) fn validate_slug(slug: &str) -> Result<String, String> {
    let s = slug.trim();
    if s.is_empty() {
        return Err("Server ID 不能為空".to_string());
    }
    if s.chars().count() > 64 {
        return Err("Server ID 太長（上限 64 字）".to_string());
    }
    let ok = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/'));
    if !ok {
        return Err(format!(
            "Server ID 只能用英數與 - _ . @ /（收到「{s}」）"
        ));
    }
    Ok(s.to_string())
}

/// 傳輸型別正規化與驗證。
pub(crate) fn validate_transport(t: &str) -> Result<String, String> {
    let v = t.trim().to_ascii_lowercase();
    match v.as_str() {
        "stdio" | "http" | "sse" => Ok(v),
        _ => Err(format!("傳輸型別只能是 stdio／http／sse（收到「{t}」）")),
    }
}

/// 依傳輸型別檢查必填欄位。
pub(crate) fn validate_input(input: &McpInput) -> Result<McpInput, String> {
    let mut out = input.clone();
    out.slug = validate_slug(&input.slug)?;
    out.transport = validate_transport(&input.transport)?;
    if out.transport == "stdio" {
        if out.command.trim().is_empty() {
            return Err("stdio 伺服器必須填 Command".to_string());
        }
    } else if out.url.trim().is_empty() {
        return Err(format!("{} 伺服器必須填 URL", out.transport));
    }
    let url = out.url.trim();
    if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!("URL 必須是 http(s)：{url}"));
    }
    // 空白的鍵值對直接丟掉（前端常留一列空的）
    out.headers.retain(|h| !h.name.trim().is_empty());
    out.env.retain(|e| !e.name.trim().is_empty());
    out.args.retain(|a| !a.trim().is_empty());
    out.name = if out.name.trim().is_empty() {
        out.slug.clone()
    } else {
        out.name.trim().to_string()
    };
    out.command = out.command.trim().to_string();
    out.url = url.to_string();
    Ok(out)
}

/// 從 `mcpServers` 的一個條目（JSON）轉成我們的輸入格式。
///
/// 兩種形狀都接受（與 `apply.rs` 讀設定檔時的規則一致）：
/// - stdio：`{command, args, env}`
/// - 遠端：`{type:"http"|"sse", url, headers}`
///
/// 這是 Deep Link 匯入用的入口；`id` 來自 `mcpServers` 的鍵（cc-switch 的協定
/// 就是這樣設計的：MCP 的 Server ID 由 config 裡的鍵決定）。
pub fn input_from_json(id: &str, v: &serde_json::Value) -> Result<McpInput, String> {
    let s = |k: &str| -> String {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let pairs = |k: &str| -> Vec<HeaderPair> {
        v.get(k)
            .and_then(|x| x.as_object())
            .map(|m| {
                m.iter()
                    .map(|(name, val)| HeaderPair {
                        name: name.clone(),
                        value: val.as_str().unwrap_or_default().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let ty = s("type");
    let transport = if ty == "http" || ty == "sse" {
        ty
    } else {
        "stdio".to_string()
    };
    let args: Vec<String> = v
        .get("args")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let input = McpInput {
        id: None,
        slug: id.to_string(),
        name: id.to_string(),
        description: String::new(),
        transport,
        command: s("command"),
        args,
        url: s("url"),
        headers: pairs("headers"),
        env: pairs("env"),
    };
    // 用同一套驗證（必填欄位、slug 合法性）過一次
    validate_input(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(slug: &str, transport: &str) -> McpInput {
        McpInput {
            slug: slug.into(),
            transport: transport.into(),
            command: "npx".into(),
            url: if transport == "stdio" {
                String::new()
            } else {
                "https://example.com/mcp".into()
            },
            ..Default::default()
        }
    }

    #[test]
    fn slug_validation_allows_real_world_names() {
        for ok in [
            "mcp-fetch",
            "node_repl",
            "context7",
            "@modelcontextprotocol/server-memory",
            "a.b.c",
        ] {
            assert!(validate_slug(ok).is_ok(), "{ok} 應該可以");
        }
        for bad in ["", "   ", "有中文", "has space", "quote\"x", "a\nb"] {
            assert!(validate_slug(bad).is_err(), "{bad:?} 應該被拒");
        }
        assert!(validate_slug(&"x".repeat(65)).is_err(), "過長要拒");
        assert_eq!(validate_slug("  pad  ").unwrap(), "pad", "要去空白");
    }

    #[test]
    fn transport_validation() {
        assert_eq!(validate_transport("STDIO").unwrap(), "stdio");
        assert_eq!(validate_transport(" Http ").unwrap(), "http");
        assert!(validate_transport("grpc").is_err());
    }

    #[test]
    fn required_fields_per_transport() {
        // stdio 要 command、不要 url
        assert!(validate_input(&input("a", "stdio")).is_ok());
        let mut no_cmd = input("a", "stdio");
        no_cmd.command = "  ".into();
        assert!(validate_input(&no_cmd).unwrap_err().contains("Command"));
        // http/sse 要 url
        assert!(validate_input(&input("a", "http")).is_ok());
        let mut no_url = input("a", "sse");
        no_url.url = String::new();
        assert!(validate_input(&no_url).unwrap_err().contains("URL"));
        // 相對網址要拒
        let mut rel = input("a", "http");
        rel.url = "/mcp".into();
        assert!(validate_input(&rel).unwrap_err().contains("http"));
    }

    #[test]
    fn empty_name_falls_back_to_slug_and_blank_pairs_are_dropped() {
        let mut i = input("my-server", "stdio");
        i.name = "   ".into();
        i.headers = vec![
            HeaderPair {
                name: "  ".into(),
                value: "x".into(),
            },
            HeaderPair {
                name: "Authorization".into(),
                value: "Bearer t".into(),
            },
        ];
        i.env = vec![HeaderPair {
            name: "".into(),
            value: "y".into(),
        }];
        i.args = vec!["  ".into(), "mcp-server-fetch".into()];
        let out = validate_input(&i).unwrap();
        assert_eq!(out.name, "my-server");
        assert_eq!(out.headers.len(), 1);
        assert_eq!(out.headers[0].name, "Authorization");
        assert!(out.env.is_empty());
        assert_eq!(out.args, vec!["mcp-server-fetch"]);
    }
}
