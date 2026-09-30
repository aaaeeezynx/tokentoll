//! `mcp::apply` 的測試：條目形狀、寫檔／移除、備份、匯入解析。
//!
//! 全部用**暫存檔**（`write_servers_at`），不碰使用者真正的 `~/.claude.json`
//! 等檔案 —— 那些留給實機驗證，而實機驗證會比對檔案雜湊確認沒留殘骸。

use super::*;
use crate::mcp::HeaderPair;

fn srv(slug: &str, transport: &str) -> McpServer {
    McpServer {
        id: 1,
        slug: slug.into(),
        name: slug.into(),
        description: String::new(),
        transport: transport.into(),
        command: if transport == "stdio" { "npx".into() } else { String::new() },
        args: if transport == "stdio" {
            vec!["-y".into(), "pkg".into()]
        } else {
            vec![]
        },
        url: if transport == "stdio" {
            String::new()
        } else {
            "https://example.com/mcp".into()
        },
        headers: vec![HeaderPair {
            name: "X-Token".into(),
            value: "t".into(),
        }],
        env: vec![HeaderPair {
            name: "KEY".into(),
            value: "v".into(),
        }],
        bindings: vec![],
    }
}

fn tmp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("cfg");
    (dir, file)
}

fn write_text(p: &Path, t: &str) {
    std::fs::write(p, t).unwrap();
}

#[test]
fn claude_entry_shapes() {
    let stdio = claude_entry(&srv("a", "stdio"));
    assert_eq!(stdio["command"], "npx");
    assert_eq!(stdio["args"][1], "pkg");
    assert_eq!(stdio["env"]["KEY"], "v");
    assert!(stdio.get("url").is_none(), "stdio 不該有 url");

    let http = claude_entry(&srv("b", "http"));
    assert_eq!(http["type"], "http");
    assert_eq!(http["url"], "https://example.com/mcp");
    assert_eq!(http["headers"]["X-Token"], "t");
    assert!(http.get("command").is_none());
}

#[test]
fn opencode_entry_shapes() {
    let local = opencode_entry(&srv("a", "stdio"));
    assert_eq!(local["type"], "local");
    // OpenCode 的 local 是「一整串指令」而不是 command ＋ args
    assert_eq!(local["command"][0], "npx");
    assert_eq!(local["command"][2], "pkg");
    assert_eq!(local["environment"]["KEY"], "v");
    assert_eq!(local["enabled"], true);

    let remote = opencode_entry(&srv("b", "sse"));
    assert_eq!(remote["type"], "remote");
    assert_eq!(remote["url"], "https://example.com/mcp");
    assert_eq!(remote["enabled"], true);
}

#[test]
fn codex_table_shapes() {
    let t = codex_table(&srv("a", "stdio"));
    assert_eq!(t["command"].as_str(), Some("npx"));
    assert_eq!(t["args"].as_array().unwrap().len(), 2);
    assert_eq!(t["env"]["KEY"].as_str(), Some("v"));
    let h = codex_table(&srv("b", "http"));
    assert_eq!(h["url"].as_str(), Some("https://example.com/mcp"));
    assert!(h.get("command").is_none());
}

#[test]
fn json_block_edit_keeps_other_keys_and_only_removes_managed() {
    let old = r#"{"otherKey":1,"mcpServers":{"keep-me":{"command":"x"},"ours-old":{"command":"y"}}}"#;
    let upserts = vec![("ours-new".to_string(), claude_entry(&srv("ours-new", "stdio")))];
    let out = edit_json_block(Some(old), "mcpServers", &upserts, &["ours-old".to_string()])
        .unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["otherKey"], 1, "其他鍵要保留");
    assert!(v["mcpServers"].get("keep-me").is_some(), "手寫的要保留");
    assert!(v["mcpServers"].get("ours-old").is_none(), "我們管的要移除");
    assert_eq!(v["mcpServers"]["ours-new"]["command"], "npx");
    assert!(out.ends_with('\n'), "結尾要有換行");
}

/// **合併而不是覆蓋**：同一個 slug 已存在時，我們沒建模的欄位不可以被吃掉。
///
/// 這條是設計時發現的資料遺失風險：使用者的 Codex 設定可能有 `cwd`、
/// `startup_timeout_sec` 之類我們沒有欄位的鍵，而「同步」是按下開關就會發生的事。
#[test]
fn json_upsert_merges_into_existing_entry() {
    let old = r#"{"mcpServers":{"ours":{"command":"old","args":["a"],"customKey":"keep-me"}}}"#;
    let upserts = vec![("ours".to_string(), claude_entry(&srv("ours", "stdio")))];
    let out = edit_json_block(Some(old), "mcpServers", &upserts, &[]).unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["mcpServers"]["ours"]["command"], "npx", "我們管的要更新");
    assert_eq!(v["mcpServers"]["ours"]["args"][1], "pkg");
    assert_eq!(
        v["mcpServers"]["ours"]["customKey"], "keep-me",
        "不是我們管的鍵必須留著（否則同步會靜默吃掉使用者的設定）"
    );
}

#[test]
fn codex_upsert_merges_into_existing_table() {
    let old = "[mcp_servers.ours]\ncommand = \"old\"\ncwd = \"C:/work\"\nstartup_timeout_sec = 30\n";
    let upserts = vec![("ours".to_string(), codex_table(&srv("ours", "stdio")))];
    let out = edit_codex_toml(Some(old), &upserts, &[]).unwrap();
    let doc: DocumentMut = out.parse().unwrap();
    let t = doc["mcp_servers"]["ours"].as_table().unwrap();
    assert_eq!(t["command"].as_str(), Some("npx"), "我們管的要更新");
    assert_eq!(t["cwd"].as_str(), Some("C:/work"), "cwd 不是我們管的，要留著");
    assert_eq!(t["startup_timeout_sec"].as_integer(), Some(30));
}

#[test]
fn json_block_edit_refuses_broken_or_wrong_shaped_files() {    // 壞掉的 JSON → 明確錯誤，不改檔案
    let e = edit_json_block(Some("{not json"), "mcpServers", &[], &[]).unwrap_err();
    assert!(e.contains("不是合法 JSON"), "{e}");
    // 最外層不是物件
    let e = edit_json_block(Some("[1,2]"), "mcpServers", &[], &[]).unwrap_err();
    assert!(e.contains("最外層不是物件"), "{e}");
    // 已有的 mcpServers 不是物件（有人手寫成陣列）
    let e = edit_json_block(Some(r#"{"mcpServers":[]}"#), "mcpServers", &[], &[]).unwrap_err();
    assert!(e.contains("不是物件"), "{e}");
}

#[test]
fn codex_toml_edit_keeps_comments_and_other_sections() {
    let old = "# 我的註解\nmodel = \"gpt\"\n\n[model_providers.custom]\nbase_url = \"http://x\"\n\n[mcp_servers.keep-me]\ncommand = \"uvx\"\n";
    let upserts = vec![("ours".to_string(), codex_table(&srv("ours", "stdio")))];
    let out = edit_codex_toml(Some(old), &upserts, &["gone".to_string()]).unwrap();
    assert!(out.contains("# 我的註解"), "註解要保留");
    assert!(out.contains("[model_providers.custom]"), "其他段要保留");
    assert!(out.contains("[mcp_servers.keep-me]"), "手寫的伺服器要保留");
    assert!(out.contains("[mcp_servers.ours]"), "新的要寫進去：\n{out}");
    assert!(out.contains("args = [\"-y\", \"pkg\"]"), "{out}");
    assert!(out.contains("[mcp_servers.ours.env]"), "env 要用子表格：\n{out}");
    // 解析回來確認合法
    let doc: DocumentMut = out.parse().expect("寫出來的 TOML 必須合法");
    assert!(doc["mcp_servers"].get("ours").is_some());
}

#[test]
fn codex_toml_edit_creates_the_section_when_missing() {
    let out = edit_codex_toml(Some("model = \"gpt\"\n"), &[], &[]).unwrap();
    assert!(out.contains("[mcp_servers]"), "沒有這個段也要建出來：\n{out}");
}

#[test]
fn write_then_remove_roundtrip_with_backup() {
    let (dir, file) = tmp();
    let app_data = dir.path().join("data");
    write_text(&file, r#"{"mcpServers":{"keep-me":{"command":"x"}}}"#);

    // 寫入
    let out = write_servers_at(
        "claude",
        &file,
        &app_data,
        &[srv("ours", "stdio")],
        &["ours".to_string()],
    )
    .unwrap();
    assert_eq!(out.written, vec!["ours"]);
    assert!(out.removed.is_empty());
    let backup = out.backup.expect("改寫前要有備份");
    assert!(PathBuf::from(&backup).exists(), "備份檔要真的存在");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(v["mcpServers"]["ours"].is_object());
    assert!(v["mcpServers"]["keep-me"].is_object());

    // 再寫一次同樣的內容 → 不重寫（不留無謂備份）
    let out2 = write_servers_at(
        "claude",
        &file,
        &app_data,
        &[srv("ours", "stdio")],
        &["ours".to_string()],
    )
    .unwrap();
    assert!(out2.backup.is_none(), "內容沒變不該再寫一次");

    // 移除（managed 有它、這次不啟用）
    let out3 = write_servers_at("claude", &file, &app_data, &[], &["ours".to_string()]).unwrap();
    assert_eq!(out3.removed, vec!["ours"]);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(v["mcpServers"].get("ours").is_none(), "要移除");
    assert!(v["mcpServers"]["keep-me"].is_object(), "手寫的還在");
}

#[test]
fn empty_sync_does_not_touch_the_file() {
    let (dir, file) = tmp();
    let app_data = dir.path().join("data");
    write_text(&file, r#"{"theme":"dark"}"#);
    let before = std::fs::read_to_string(&file).unwrap();
    // 沒有伺服器、也沒有我們管的東西 → 連開頭的空區塊都不該被寫進去
    let out = write_servers_at("opencode", &file, &app_data, &[], &[]).unwrap();
    assert!(out.written.is_empty() && out.removed.is_empty());
    assert!(out.backup.is_none());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        before,
        "沒事不該動檔案（實機驗證時真的留下了空的 mcp 區塊）"
    );

    // **我們管著某個 slug、但檔案裡沒有它**（例如只綁到別的工具有）→ 一樣不該動
    write_text(&file, "{\n  \"theme\": \"dark\"\n}\n");
    let before = std::fs::read_to_string(&file).unwrap();
    let out = write_servers_at(
        "opencode",
        &file,
        &app_data,
        &[],
        &["managed-elsewhere".to_string()],
    )
    .unwrap();
    assert!(out.backup.is_none() && out.written.is_empty() && out.removed.is_empty());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        before,
        "檔案裡沒有我們管的東西，就不該改它"
    );
}

#[test]
fn removing_the_last_entry_drops_the_empty_block() {
    let (dir, file) = tmp();
    let app_data = dir.path().join("data");
    write_text(&file, r#"{"theme":"dark","mcpServers":{"ours":{"command":"x"}}}"#);
    let out = write_servers_at("claude", &file, &app_data, &[], &["ours".to_string()]).unwrap();
    assert_eq!(out.removed, vec!["ours"]);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(v.get("mcpServers").is_none(), "空了就把鍵拿掉：{v}");
    assert_eq!(v["theme"], "dark", "其他鍵照舊");
}

/// 「開了又關」要變成真正的 no-op：檔案不能被我們的重新序列化弄變形。
#[test]
fn add_then_remove_returns_the_original_text() {
    let original = "{\n  \"theme\": \"dark\",\n  \"nested\": {\n    \"a\": 1\n  }\n}\n";
    let upserts = vec![("ours".to_string(), claude_entry(&srv("ours", "stdio")))];
    let added = edit_json_block(Some(original), "mcpServers", &upserts, &[]).unwrap();
    assert!(added.contains("mcpServers"));
    // 再移除 → 應該與原文一模一樣（含排版）
    let back = edit_json_block(Some(&added), "mcpServers", &[], &["ours".to_string()]).unwrap();
    assert_eq!(back, original, "來回操作後必須與原文逐字相同");
}

#[test]
fn codex_add_then_remove_returns_to_the_original_content() {
    let original = "# 註解\nmodel = \"gpt\"\n\n[mcp_servers.keep]\ncommand = \"uvx\"\n";
    let upserts = vec![("ours".to_string(), codex_table(&srv("ours", "stdio")))];
    let added = edit_codex_toml(Some(original), &upserts, &[]).unwrap();
    assert!(added.contains("[mcp_servers.ours]"));
    let back = edit_codex_toml(Some(&added), &[], &["ours".to_string()]).unwrap();
    // toml_edit 只重寫我們動到的表格，其他段落逐字保留 ——
    // 所以移除後應該與原文**完全相同**（含註解與排版）。
    assert_eq!(back, original, "TOML 這邊應該逐字回到原狀");
}

#[test]
fn write_creates_file_when_missing() {
    let (dir, file) = tmp();
    let app_data = dir.path().join("data");
    assert!(!file.exists());
    let out = write_servers_at(
        "opencode",
        &file,
        &app_data,
        &[srv("ours", "stdio")],
        &["ours".to_string()],
    )
    .unwrap();
    assert!(out.backup.is_none(), "本來沒有檔案就沒有東西好備份");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(v["mcp"]["ours"]["type"], "local");
}

#[test]
fn backups_are_rotated() {
    let (dir, file) = tmp();
    let app_data = dir.path().join("data");
    let bdir = app_data.join("backups").join("claude-mcp");
    for i in 0..8 {
        write_text(&file, &format!(r#"{{"mcpServers":{{"n{i}":{{"command":"x"}}}}}}"#));
        write_servers_at(
            "claude",
            &file,
            &app_data,
            &[srv("ours", "stdio")],
            &["ours".to_string()],
        )
        .unwrap();
    }
    let n = std::fs::read_dir(&bdir).unwrap().count();
    assert!(
        n <= MCP_BACKUP_KEEP,
        "備份要輪換（留 {MCP_BACKUP_KEEP} 份），實際 {n}"
    );
}

// ── 匯入解析 ──

#[test]
fn read_claude_servers() {
    let text = r#"{"mcpServers":{
        "fetch":{"command":"uvx","args":["mcp-server-fetch"],"env":{"K":"v"}},
        "remote":{"type":"http","url":"https://x/mcp","headers":{"A":"B"}},
        "broken":{}}} "#;
    let got = read_servers("claude", Some(text)).unwrap();
    assert_eq!(got.len(), 2, "沒有 command／url 的項目要跳過：{got:?}");
    let fetch = got.iter().find(|s| s.slug == "fetch").unwrap();
    assert_eq!(fetch.transport, "stdio");
    assert_eq!(fetch.command, "uvx");
    assert_eq!(fetch.args, vec!["mcp-server-fetch"]);
    assert_eq!(fetch.env[0].name, "K");
    let remote = got.iter().find(|s| s.slug == "remote").unwrap();
    assert_eq!(remote.transport, "http");
    assert_eq!(remote.url, "https://x/mcp");
    assert_eq!(remote.headers[0].value, "B");
}

/// 使用者機器上的真實形狀：`[mcp_servers.node_repl]` 帶一個 `.env` 子表格 ——
/// 子表格不可以被當成另一個伺服器。
#[test]
fn read_codex_servers_ignores_env_subtable() {
    let text = r#"
model = "gpt"

[mcp_servers.node_repl]
command = "node_repl.exe"
args = ["-x"]

[mcp_servers.node_repl.env]
CODEX_HOME = "C:\\Users\\x\\.codex"

[mcp_servers.remote_one]
url = "https://y/mcp"
"#;
    let got = read_servers("codex", Some(text)).unwrap();
    let slugs: Vec<&str> = got.iter().map(|s| s.slug.as_str()).collect();
    assert_eq!(slugs, vec!["node_repl", "remote_one"], "不該多出 env");
    let nr = &got[0];
    assert_eq!(nr.command, "node_repl.exe");
    assert_eq!(nr.args, vec!["-x"]);
    assert_eq!(nr.env.len(), 1);
    assert_eq!(nr.env[0].name, "CODEX_HOME");
    let ro = &got[1];
    assert_eq!(ro.transport, "http");
    assert_eq!(ro.url, "https://y/mcp");
}

#[test]
fn read_opencode_servers() {
    let text = r#"{"mcp":{
      "local-one":{"type":"local","command":["npx","-y","pkg"],"environment":{"K":"v"},"enabled":true},
      "remote-one":{"type":"remote","url":"https://z/mcp","enabled":false}}}"#;
    let got = read_servers("opencode", Some(text)).unwrap();
    assert_eq!(got.len(), 2);
    let l = got.iter().find(|s| s.slug == "local-one").unwrap();
    assert_eq!(l.command, "npx");
    assert_eq!(l.args, vec!["-y", "pkg"]);
    assert_eq!(l.env[0].value, "v");
    let r = got.iter().find(|s| s.slug == "remote-one").unwrap();
    assert_eq!(r.transport, "http");
    assert_eq!(r.url, "https://z/mcp");
}

#[test]
fn read_handles_missing_or_broken_files() {
    assert!(read_servers("claude", None).unwrap().is_empty(), "檔案不存在＝沒有");
    assert!(read_servers("claude", Some("  ")).unwrap().is_empty());
    assert!(read_servers("claude", Some("{}")).unwrap().is_empty(), "沒有那個鍵");
    // 壞掉 → 明確錯誤（呼叫端要回報「這個工具匯入失敗」）
    assert!(read_servers("claude", Some("{oops")).unwrap_err().contains("JSON"));
    assert!(read_servers("codex", Some("= =")).unwrap_err().contains("TOML"));
    // 不支援的工具
    assert!(read_servers("cursor", Some("{}")).is_err());
    assert!(read_servers("claude", Some(r#"{"mcpServers":[1]}"#)).unwrap().is_empty());
}
