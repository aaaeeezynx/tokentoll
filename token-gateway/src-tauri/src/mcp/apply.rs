//! 把 MCP 伺服器寫進（或移出）各工具的設定檔。
//!
//! ## 三個工具的位置與形狀（都有出處）
//!
//! | 工具 | 檔案 | 區塊 |
//! |---|---|---|
//! | Claude Code | `~/.claude.json` | `mcpServers`（**不是** `settings.json`） |
//! | Codex | `~/.codex/config.toml` | `[mcp_servers]` |
//! | OpenCode | `~/.config/opencode/opencode.json` | `mcp` |
//!
//! - Claude 的 stdio 項目是 `{command, args, env}`；http／sse 是
//!   `{type:"http"|"sse", url, headers}`。
//! - Codex 的 stdio 是 `command` ＋ `args` ＋ `[mcp_servers.X.env]`；
//!   streamable HTTP 是 `url` —— 這一條是**用 `codex mcp add --help` 問出來的**
//!   （`--url <URL> | -- <COMMAND>...`），不是猜的。
//! - OpenCode 是 `{type:"local", command:[...], environment:{...}, enabled:true}`
//!   或 `{type:"remote", url, headers, enabled:true}`。
//!
//! ## 安全規則
//!
//! 1. **只動我們管的鍵**：移除時只刪「資料庫裡有、但這次不該啟用」的 slug；
//!    使用者手寫的其他伺服器一律原樣保留（cc-switch 也是這條規則）。
//! 2. **改寫前先備份**：備份放 `<app_data>/backups/<app>-mcp/`，**與接管的備份分開**
//!    ——接管的輪換只留 1 份，混在一起會把「接管前」的基準備份擠掉。
//! 3. **原子寫入**：沿用 `fsutil::atomic_write`（與接管同一條路徑）。
//! 4. **原檔壞掉就不寫**：JSON／TOML 解析失敗時回錯誤、不動檔案。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use toml_edit::{Array, DocumentMut, Item, Table};

use super::{HeaderPair, McpInput, McpServer};

/// MCP 備份保留份數（比接管的 1 份寬鬆：這裡動的是伺服器清單，不是連線設定）。
pub(crate) const MCP_BACKUP_KEEP: usize = 5;

/// 某個工具的**設定檔**路徑。
pub fn app_config_path(app: &str) -> Result<PathBuf, String> {
    let home = crate::tools::user_home()?;
    match app {
        "claude" => Ok(home.join(".claude.json")),
        "codex" => Ok(home.join(".codex").join("config.toml")),
        "opencode" => Ok(home.join(".config").join("opencode").join("opencode.json")),
        _ => Err(format!("MCP 不支援的工具：{app}")),
    }
}

/// 這個工具有沒有安裝 —— cc-switch 的同步條件（沒裝就不寫、也不報錯）。
pub(crate) fn app_installed(app: &str) -> bool {
    let home = match crate::tools::user_home() {
        Ok(h) => h,
        Err(_) => return false,
    };
    match app {
        // cc-switch：`~/.claude/` 目錄或 `~/.claude.json` 檔案存在即可
        "claude" => home.join(".claude.json").exists() || home.join(".claude").exists(),
        "codex" => home.join(".codex").exists(),
        "opencode" => home.join(".config").join("opencode").exists(),
        _ => false,
    }
}

/// 一次寫入的結果（給 UI 顯示與測試斷言）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteOutcome {
    pub path: String,
    /// 這次寫進去（新增或更新）的 slug
    pub written: Vec<String>,
    /// 這次從設定檔移除的 slug
    pub removed: Vec<String>,
    pub backup: Option<String>,
}

// ------------------------------------------------------------ 條目轉換 ---

fn pairs_to_map(pairs: &[HeaderPair]) -> Map<String, Value> {
    let mut m = Map::new();
    for p in pairs {
        if !p.name.trim().is_empty() {
            m.insert(p.name.trim().to_string(), Value::String(p.value.clone()));
        }
    }
    m
}

/// Claude Code 的 `mcpServers.<slug>` 條目。
pub(crate) fn claude_entry(s: &McpServer) -> Value {
    if s.transport == "stdio" {
        let mut o = Map::new();
        o.insert("command".into(), Value::String(s.command.clone()));
        if !s.args.is_empty() {
            o.insert(
                "args".into(),
                Value::Array(s.args.iter().cloned().map(Value::String).collect()),
            );
        }
        let env = pairs_to_map(&s.env);
        if !env.is_empty() {
            o.insert("env".into(), Value::Object(env));
        }
        Value::Object(o)
    } else {
        let mut o = Map::new();
        o.insert("type".into(), Value::String(s.transport.clone()));
        o.insert("url".into(), Value::String(s.url.clone()));
        let h = pairs_to_map(&s.headers);
        if !h.is_empty() {
            o.insert("headers".into(), Value::Object(h));
        }
        Value::Object(o)
    }
}

/// OpenCode 的 `mcp.<slug>` 條目。
pub(crate) fn opencode_entry(s: &McpServer) -> Value {
    let mut o = Map::new();
    if s.transport == "stdio" {
        let mut cmd = vec![Value::String(s.command.clone())];
        cmd.extend(s.args.iter().cloned().map(Value::String));
        o.insert("type".into(), Value::String("local".into()));
        o.insert("command".into(), Value::Array(cmd));
        let env = pairs_to_map(&s.env);
        if !env.is_empty() {
            o.insert("environment".into(), Value::Object(env));
        }
    } else {
        o.insert("type".into(), Value::String("remote".into()));
        o.insert("url".into(), Value::String(s.url.clone()));
        let h = pairs_to_map(&s.headers);
        if !h.is_empty() {
            o.insert("headers".into(), Value::Object(h));
        }
    }
    o.insert("enabled".into(), Value::Bool(true));
    Value::Object(o)
}

/// Codex 的 `[mcp_servers.<slug>]` 表格。
pub(crate) fn codex_table(s: &McpServer) -> Table {
    let mut t = Table::new();
    if s.transport == "stdio" {
        t.insert("command", toml_edit::value(s.command.clone()));
        if !s.args.is_empty() {
            let mut a = Array::new();
            for x in &s.args {
                a.push(x.clone());
            }
            t.insert("args", toml_edit::value(a));
        }
        let env = pairs_to_map(&s.env);
        if !env.is_empty() {
            let mut et = Table::new();
            for (k, v) in env {
                et.insert(&k, toml_edit::value(v.as_str().unwrap_or_default()));
            }
            t.insert("env", Item::Table(et));
        }
    } else {
        t.insert("url", toml_edit::value(s.url.clone()));
    }
    t
}

// ------------------------------------------------------------ 寫檔 ---

/// 改寫一個「鍵 → 條目」的 JSON 區塊（Claude 的 `mcpServers`／OpenCode 的 `mcp`）。
///
/// `removals` 只會移除**我們管的** slug（呼叫端負責篩選）。
pub(crate) fn edit_json_block(
    text: Option<&str>,
    key: &str,
    upserts: &[(String, Value)],
    removals: &[String],
) -> Result<String, String> {
    let mut root: Value = match text {
        Some(t) if !t.trim().is_empty() => {
            serde_json::from_str(t).map_err(|e| format!("設定檔不是合法 JSON（{e}）"))?
        }
        _ => Value::Object(Map::new()),
    };
    let original = root.clone();
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "設定檔最外層不是物件".to_string())?;
    let entry = obj
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        return Err(format!("設定檔的 {key} 不是物件，不敢改"));
    }
    let map = entry.as_object_mut().expect("just checked");
    for slug in removals {
        map.remove(slug);
    }
    for (slug, v) in upserts {
        // **合併而不是覆蓋**：同一個 slug 已經存在時，只更新我們管的欄位
        // （command／args／url／env／headers／type），其他鍵原樣留著。
        //
        // 為什麼：使用者的設定檔可能有我們沒有建模的欄位（Codex 的 `cwd`、
        // `startup_timeout_sec` 之類）。如果直接覆蓋，那些欄位會在「同步」時
        // 被無聲吃掉 —— 而同步是按下開關就會發生的事，代價太大。
        if let (Some(existing_obj), Some(our_obj)) = (map.get_mut(slug), v.as_object()) {
            if let Some(e) = existing_obj.as_object_mut() {
                for (k, val) in our_obj {
                    e.insert(k.clone(), val.clone());
                }
                continue;
            }
        }
        map.insert(slug.clone(), v.clone());
    }
    // 移除後如果整個區塊空了，就把鍵拿掉 —— 不要在使用者的檔案裡留下
    // `"mcpServers": {}` 這種我們造成的空殼（他原本可能根本沒有這個鍵）。
    if map.is_empty() {
        obj.remove(key);
    }
    // **值沒變就原樣回傳**：把「開了又關」這種來回操作變成真正的 no-op，
    // 檔案一個位元組都不會被動到（我們的重新序列化會把排版正規化，
    // 不該因為一次無效的切換就讓使用者的檔案整份變形）。
    if root == original {
        return Ok(text.unwrap_or_default().to_string());
    }
    let mut out = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

/// 改寫 Codex 的 `[mcp_servers]` 段（用 `toml_edit` 保留註解與其他段）。
pub(crate) fn edit_codex_toml(
    text: Option<&str>,
    upserts: &[(String, Table)],
    removals: &[String],
) -> Result<String, String> {
    let mut doc: DocumentMut = match text {
        Some(t) if !t.trim().is_empty() => {
            t.parse().map_err(|e| format!("config.toml 不是合法 TOML（{e}）"))?
        }
        _ => DocumentMut::new(),
    };
    // 沒有 mcp_servers 就建一個（顯式表格，寫出來的 TOML 才有 [mcp_servers] 段名）
    if doc.get("mcp_servers").is_none() {
        let mut t = Table::new();
        t.set_implicit(false);
        doc.insert("mcp_servers", Item::Table(t));
    }
    let servers = doc["mcp_servers"]
        .as_table_mut()
        .ok_or_else(|| "config.toml 的 mcp_servers 不是表格，不敢改".to_string())?;
    for slug in removals {
        servers.remove(slug);
    }
    for (slug, table) in upserts {
        // **合併而不是覆蓋**：已經存在的話，只更新我們管的鍵
        // （command／args／url／env），其他鍵（例如 Codex 的 `cwd`、
        // `startup_timeout_sec`）原樣保留 —— 那些是使用者的設定，
        // 不該因為我們按了一次同步就消失。
        if let Some(existing) = servers.get_mut(slug).and_then(|i| i.as_table_mut()) {
            for (k, v) in table.iter() {
                existing.insert(k, v.clone());
            }
            continue;
        }
        servers.insert(slug, Item::Table(table.clone()));
    }
    // 註：TOML 這邊**刻意不做**「值沒變就原樣回傳」的最佳化 —— `toml_edit` 本來就
    // 只重寫我們動到的表格（其他段落的排版與註解逐字保留），所以沒有 JSON 那邊
    // 「整份重新序列化」的變形問題。
    Ok(doc.to_string())
}

/// 寫入前備份（放 `<app_data>/backups/<app>-mcp/`，與接管的備份分開）。
fn backup_file(app_data: &Path, app: &str, file: &Path, text: &str) -> Option<String> {
    let dir = app_data.join("backups").join(format!("{app}-mcp"));
    std::fs::create_dir_all(&dir).ok()?;
    let stem = file.file_name()?.to_str()?;
    let name = format!("{stem}.bak-{}", crate::fsutil::backup_stamp());
    let dest = crate::fsutil::unique_backup_name(&dir, &name);
    std::fs::write(&dest, text).ok()?;
    prune_backups(&dir, MCP_BACKUP_KEEP);
    Some(dest.to_string_lossy().to_string())
}

/// 只保留最近 N 份 MCP 備份（依檔名排序＝時間序，`backup_stamp` 可排序）。
fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    if files.len() <= keep {
        return;
    }
    for p in &files[..files.len() - keep] {
        let _ = std::fs::remove_file(p);
    }
}

/// 讀設定檔目前的內容（不存在就 `None`）。
fn read_current(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|e| format!("讀取 {} 失敗：{e}", path.display()))
}

/// 檔案裡有沒有「我們管的」slug（用來判斷「沒事就不該動檔案」）。
///
/// 讀不懂的檔案一律回 `true`：那代表我們**可能**動得了它，讓後續流程去面對
/// （解析失敗時 `edit_*` 會回明確錯誤，而不是靜默略過）。
fn contains_any_managed(app: &str, text: Option<&str>, managed: &[String]) -> bool {
    if managed.is_empty() {
        return false;
    }
    let Some(text) = text else {
        return false;
    };
    let Ok(found) = read_servers(app, Some(text)) else {
        return true;
    };
    found
        .iter()
        .any(|s| managed.iter().any(|m| m == &s.slug))
}

/// 把「這個工具該有哪些伺服器」寫進它的設定檔。
///
/// - `servers`：這次要啟用的伺服器（會新增或覆蓋）
/// - `managed`：資料庫裡全部 slug —— 只有清單裡、且不在 `servers` 的才會被移除
///
/// 呼叫端負責解析路徑（`app_config_path`）並在工具沒安裝時跳過。
pub(crate) fn write_servers_at(
    app: &str,
    path: &Path,
    app_data: &Path,
    servers: &[McpServer],
    managed: &[String],
) -> Result<WriteOutcome, String> {
    let existing = read_current(path)?;
    // **沒事就不碰檔案**：沒有東西要寫，而且檔案裡也沒有我們管的 slug 可移除時，
    // 直接回報「無變動」。少了這一條，一次「重新同步」會在所有工具的設定檔裡
    // 留下空的 `"mcp": {}` / `[mcp_servers]` 區塊（實機驗證時真的發生了），
    // 甚至可能因為排版差異而白改一個我們根本不該動的檔案。
    if servers.is_empty()
        && !contains_any_managed(app, existing.as_deref(), managed)
    {
        return Ok(WriteOutcome {
            path: path.to_string_lossy().to_string(),
            written: vec![],
            removed: vec![],
            backup: None,
        });
    }
    let managed_set: std::collections::HashSet<&str> =
        managed.iter().map(|s| s.as_str()).collect();
    // 這次要啟用的 slug
    let want: std::collections::HashSet<&str> =
        servers.iter().map(|s| s.slug.as_str()).collect();
    // 移除＝「我們管的、但這次不該有」的
    let removals: Vec<String> = managed_set
        .iter()
        .filter(|s| !want.contains(**s))
        .map(|s| (*s).to_string())
        .collect();

    let new_text = match app {
        "claude" => {
            let upserts: Vec<(String, Value)> = servers
                .iter()
                .map(|s| (s.slug.clone(), claude_entry(s)))
                .collect();
            edit_json_block(existing.as_deref(), "mcpServers", &upserts, &removals)?
        }
        "opencode" => {
            let upserts: Vec<(String, Value)> = servers
                .iter()
                .map(|s| (s.slug.clone(), opencode_entry(s)))
                .collect();
            edit_json_block(existing.as_deref(), "mcp", &upserts, &removals)?
        }
        "codex" => {
            let upserts: Vec<(String, Table)> = servers
                .iter()
                .map(|s| (s.slug.clone(), codex_table(s)))
                .collect();
            edit_codex_toml(existing.as_deref(), &upserts, &removals)?
        }
        _ => return Err(format!("MCP 不支援的工具：{app}")),
    };

    // 內容沒變就不要動檔案（避免無謂的備份與 mtime 變動）
    if existing.as_deref() == Some(new_text.as_str()) {
        return Ok(WriteOutcome {
            path: path.to_string_lossy().to_string(),
            written: vec![],
            removed: vec![],
            backup: None,
        });
    }
    let backup = existing
        .as_deref()
        .and_then(|old| backup_file(app_data, app, path, old));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("建立目錄失敗：{e}"))?;
    }
    crate::fsutil::atomic_write(path, new_text.as_bytes())
        .map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
    Ok(WriteOutcome {
        path: path.to_string_lossy().to_string(),
        written: servers.iter().map(|s| s.slug.clone()).collect(),
        removed: removals,
        backup,
    })
}

// ------------------------------------------------------------ 讀檔（匯入） ---

/// 從某個工具的設定檔讀出所有 MCP 伺服器（匯入用）。
///
/// 讀不懂（檔案壞掉）時回 `Err`，由呼叫端回報「這個工具匯入失敗、原因為何」——
/// cc-switch 的匯入報告也是這樣（部分成功時會列出失敗的 app 與原因）。
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

/// 便利函式：把 `Value::Null` 轉成「沒有內容」的字串（測試用）。
#[cfg(test)]
mod tests;
