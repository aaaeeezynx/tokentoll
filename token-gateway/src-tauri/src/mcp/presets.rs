//! MCP 預設樣板（P3.1，對標 cc-switch 的 preset templates）。
//!
//! 套件名照抄 cc-switch 手冊 §3.1 的「Common Presets」表（這是唯一有出處的部分）；
//! **啟動指令是我們的判斷**：官方 Python 伺服器（fetch）用 `uvx`，
//! TypeScript 套件用 `npx -y`。樣板的說明欄會寫出這一點，並提醒使用者按需要改 ——
//! 沒有安裝對應指令時伺服器起不來（cc-switch 的 FAQ 也列了同一條）。

use serde::Serialize;

#[cfg(test)]
use super::McpInput;

#[derive(Debug, Clone, Serialize)]
pub struct McpPreset {
    pub id: &'static str,
    pub name: &'static str,
    /// 套件名（照抄 cc-switch 的表）
    pub package: &'static str,
    pub description: &'static str,
    /// 說明：這個樣板預期什麼、可能要改什麼
    pub note: &'static str,
    pub input: PresetInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetInput {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
}

/// 表格的一列：(slug, 顯示名, 套件, 說明, 啟動指令, 參數)。
type PresetRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const TABLE: &[PresetRow] = &[
    // (slug, 顯示名, 套件, 說明, 啟動指令, 參數)
    (
        "fetch",
        "Fetch",
        "mcp-server-fetch",
        "讓 AI 抓取網頁內容",
        "uvx",
        &["mcp-server-fetch"],
    ),
    (
        "time",
        "Time",
        "@modelcontextprotocol/server-time",
        "提供目前時間",
        "npx",
        &["-y", "@modelcontextprotocol/server-time"],
    ),
    (
        "memory",
        "Memory",
        "@modelcontextprotocol/server-memory",
        "讓 AI 記住與取回資訊",
        "npx",
        &["-y", "@modelcontextprotocol/server-memory"],
    ),
    (
        "sequential-thinking",
        "Sequential Thinking",
        "@modelcontextprotocol/server-sequential-thinking",
        "連鎖思考，強化推理",
        "npx",
        &["-y", "@modelcontextprotocol/server-sequential-thinking"],
    ),
    (
        "context7",
        "Context7",
        "@upstash/context7-mcp",
        "查詢技術文件",
        "npx",
        &["-y", "@upstash/context7-mcp"],
    ),
];

/// 全部預設樣板。
pub fn presets() -> Vec<McpPreset> {
    TABLE
        .iter()
        .map(|(slug, name, package, desc, cmd, args)| McpPreset {
            id: slug,
            name,
            package,
            description: desc,
            note: match *cmd {
                "uvx" => "用 uvx 啟動（需先裝 uv／Python 工具鏈）。",
                _ => "用 npx -y 啟動（需先裝 Node.js）。",
            },
            input: PresetInput {
                slug: (*slug).to_string(),
                name: (*name).to_string(),
                description: (*desc).to_string(),
                transport: "stdio".to_string(),
                command: (*cmd).to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                url: String::new(),
            },
        })
        .collect()
}

/// 樣板 → 可儲存的輸入（只填連線資訊；binding 由使用者自己勾）。
#[cfg(test)]
pub fn preset_to_input(id: &str) -> Option<McpInput> {
    let p = presets().into_iter().find(|p| p.id == id)?;
    Some(McpInput {
        id: None,
        slug: p.input.slug,
        name: p.input.name,
        description: p.input.description,
        transport: p.input.transport,
        command: p.input.command,
        args: p.input.args,
        url: p.input.url,
        headers: vec![],
        env: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::validate_input;

    #[test]
    fn presets_are_well_formed_and_storable() {
        let list = presets();
        assert_eq!(list.len(), 5, "cc-switch 手冊列了五個常見預設");
        let mut ids: Vec<&str> = list.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "樣板 id 有重複");
        for p in &list {
            assert!(!p.name.is_empty());
            assert!(!p.package.is_empty(), "{}：要寫出套件名", p.id);
            assert!(!p.note.is_empty(), "{}：要說明啟動前提", p.id);
            // 每個樣板都要能通過真正的驗證（不然使用者一按就報錯）
            let out = validate_input(&preset_to_input(p.id).unwrap()).unwrap();
            assert_eq!(out.transport, "stdio");
            assert!(!out.command.is_empty());
            assert!(!out.args.is_empty());
        }
    }

    /// 套件名要與 cc-switch 手冊一致（這是唯一有出處的部分，不許漂移）。
    #[test]
    fn packages_match_the_documented_table() {
        let by = |id: &str| {
            presets()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap()
                .package
        };
        assert_eq!(by("fetch"), "mcp-server-fetch");
        assert_eq!(by("time"), "@modelcontextprotocol/server-time");
        assert_eq!(by("memory"), "@modelcontextprotocol/server-memory");
        assert_eq!(
            by("sequential-thinking"),
            "@modelcontextprotocol/server-sequential-thinking"
        );
        assert_eq!(by("context7"), "@upstash/context7-mcp");
    }

    #[test]
    fn unknown_preset_is_none() {
        assert!(preset_to_input("nope").is_none());
    }
}
