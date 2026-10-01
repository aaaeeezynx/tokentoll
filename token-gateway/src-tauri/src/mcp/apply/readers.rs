//! 從各工具的設定檔讀回既有的 MCP 伺服器（`apply.rs` 拆出來的）。

use serde_json::{Map, Value};

use super::super::{HeaderPair, McpInput};
use super::*;

pub(crate) fn read_servers(app: &str, text: Option<&str>) -> Result<Vec<McpInput>, String> {
    let Some(text) = text else {
        return Ok(vec![]);
    };
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    match app {
        "claude" => {
            let root: Value =
                serde_json::from_str(text).map_err(|e| format!("不是合法 JSON（{e}）"))?;
            let map = root.get("mcpServers").and_then(|v| v.as_object());
            Ok(map.map(json_servers).unwrap_or_default())
        }
        "opencode" => {
            let root: Value =
                serde_json::from_str(text).map_err(|e| format!("不是合法 JSON（{e}）"))?;
            let Some(map) = root.get("mcp").and_then(|v| v.as_object()) else {
                return Ok(vec![]);
            };
            let mut out = vec![];
            for (slug, v) in map {
                let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("local");
                let input = if ty == "remote" {
                    McpInput {
                        slug: slug.clone(),
                        name: slug.clone(),
                        transport: "http".into(),
                        url: v.get("url").and_then(|u| u.as_str()).unwrap_or("").into(),
                        headers: json_pairs(v.get("headers")),
                        ..Default::default()
                    }
                } else {
                    let cmd = v.get("command").and_then(|c| c.as_array());
                    let mut parts: Vec<String> = cmd
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let command = if parts.is_empty() {
                        String::new()
                    } else {
                        parts.remove(0)
                    };
                    McpInput {
                        slug: slug.clone(),
                        name: slug.clone(),
                        transport: "stdio".into(),
                        command,
                        args: parts,
                        env: json_pairs(v.get("environment")),
                        ..Default::default()
                    }
                };
                out.push(input);
            }
            Ok(out)
        }
        "codex" => {
            let doc: DocumentMut = text
                .parse()
                .map_err(|e| format!("不是合法 TOML（{e}）"))?;
            let Some(table) = doc.get("mcp_servers").and_then(|i| i.as_table()) else {
                return Ok(vec![]);
            };
            let mut out = vec![];
            for (slug, item) in table.iter() {
                // `[mcp_servers.X.env]` 是 X 的子表格，不會在這裡單獨出現
                let Some(t) = item.as_table() else { continue };
                let command = t.get("command").and_then(|c| c.as_str()).unwrap_or("");
                let url = t.get("url").and_then(|u| u.as_str()).unwrap_or("");
                if command.is_empty() && url.is_empty() {
                    continue; // 不是伺服器（例如只有 env 的殘段）
                }
                let args: Vec<String> = t
                    .get("args")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let env = t
                    .get("env")
                    .and_then(|e| e.as_table())
                    .map(|e| {
                        e.iter()
                            .filter_map(|(k, v)| {
                                v.as_str().map(|s| HeaderPair {
                                    name: k.to_string(),
                                    value: s.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(McpInput {
                    slug: slug.to_string(),
                    name: slug.to_string(),
                    transport: if command.is_empty() { "http" } else { "stdio" }.into(),
                    command: command.to_string(),
                    args,
                    url: url.to_string(),
                    env,
                    ..Default::default()
                });
            }
            Ok(out)
        }
        _ => Err(format!("MCP 不支援的工具：{app}")),
    }
}

fn json_servers(map: &Map<String, Value>) -> Vec<McpInput> {
    let mut out = vec![];
    for (slug, v) in map {
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty == "http" || ty == "sse" {
            out.push(McpInput {
                slug: slug.clone(),
                name: slug.clone(),
                transport: ty.to_string(),
                url: v.get("url").and_then(|u| u.as_str()).unwrap_or("").into(),
                headers: json_pairs(v.get("headers")),
                ..Default::default()
            });
            continue;
        }
        let command = v.get("command").and_then(|c| c.as_str()).unwrap_or("");
        if command.is_empty() {
            continue;
        }
        let args: Vec<String> = v
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        out.push(McpInput {
            slug: slug.clone(),
            name: slug.clone(),
            transport: "stdio".into(),
            command: command.to_string(),
            args,
            env: json_pairs(v.get("env")),
            ..Default::default()
        });
    }
    out
}

fn json_pairs(v: Option<&Value>) -> Vec<HeaderPair> {
    v.and_then(|x| x.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| HeaderPair {
                    name: k.clone(),
                    value: v.as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}
