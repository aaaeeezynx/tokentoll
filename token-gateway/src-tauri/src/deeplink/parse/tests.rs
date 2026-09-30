//! `deeplink::parse` 的測試。
//!
//! 幾個案例**直接照抄 cc-switch 手冊裡的範例連結**（含它示範的 Base64 內容），
//! 這樣「我們的解析器讀得懂別人的連結」就是有出處的驗證，而不是自己編的。

use super::*;

/// cc-switch 手冊裡「Import MCP Server」的範例（原文照抄）。
const DOC_MCP: &str = "ccswitch://v1/import?resource=mcp&apps=claude,codex&config=eyJtY3BTZXJ2ZXJzIjp7Im1jcC1mZXRjaCI6eyJjb21tYW5kIjoidXZ4IiwiYXJncyI6WyJtY3Atc2VydmVyLWZldGNoIl19fX0%3D";

/// 我們的 scheme（其餘照抄）。
fn ours(rest: &str) -> String {
    format!("tokengateway://{rest}")
}

#[test]
fn percent_decode_handles_escapes_and_plus() {
    assert_eq!(percent_decode("a%20b"), "a b");
    assert_eq!(percent_decode("a+b"), "a b");
    assert_eq!(percent_decode("%E4%BB%A3%E7%A2%BC"), "代碼");
    assert_eq!(percent_decode("%2B"), "+");
    assert_eq!(percent_decode("%3D"), "=");
    // 壞掉的 escape 不該吃掉字元
    assert_eq!(percent_decode("100%"), "100%");
    assert_eq!(percent_decode("%ZZ"), "%ZZ");
}

#[test]
fn base64_tolerates_url_safe_and_missing_padding() {
    let wanted = r#"{"mcpServers":{"mcp-fetch":{"command":"uvx","args":["mcp-server-fetch"]}}}"#;
    let std = "eyJtY3BTZXJ2ZXJzIjp7Im1jcC1mZXRjaCI6eyJjb21tYW5kIjoidXZ4IiwiYXJncyI6WyJtY3Atc2VydmVyLWZldGNoIl19fX0=";
    assert_eq!(b64_decode(std).unwrap(), wanted);
    // 少了結尾的 '='（分享連結常被截）
    assert_eq!(b64_decode(std.trim_end_matches('=')).unwrap(), wanted);
    // URL-safe 字元
    let url_safe = std.replace('+', "-").replace('/', "_");
    assert_eq!(b64_decode(&url_safe).unwrap(), wanted);
    assert!(b64_decode("!!!").is_err());
}

/// **cc-switch 手冊的 MCP 範例**：改 scheme 後我們的解析器要完全讀懂。
#[test]
fn parses_the_documented_mcp_example() {
    let url = ours(DOC_MCP.strip_prefix("ccswitch://").unwrap());
    let req = parse(&url).unwrap();
    let ImportRequest::Mcp(m) = &req else {
        panic!("應該是 mcp：{req:?}");
    };
    assert_eq!(m.apps, vec!["claude".to_string(), "codex".to_string()]);
    assert_eq!(m.servers.len(), 1);
    let (id, input) = &m.servers[0];
    assert_eq!(id, "mcp-fetch", "Server ID 來自 config 的鍵");
    assert_eq!(input.transport, "stdio");
    assert_eq!(input.command, "uvx");
    assert_eq!(input.args, vec!["mcp-server-fetch"]);
    // 預覽要提醒使用者「會執行指令」
    let pv = preview(&req);
    assert_eq!(pv.kind, "mcp");
    assert!(pv.warnings.iter().any(|w| w.contains("執行指令")));
    assert!(pv.fields.iter().any(|(k, _)| k == "同步到"));
}

/// 手冊的 provider 範例（URL-encoded 的名稱與 endpoint）。
#[test]
fn parses_provider_link_with_encoded_values() {
    let url = ours(
        "v1/import?resource=provider&app=claude&name=My%20Provider\
         &endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-abcdef123456789",
    );
    let req = parse(&url).unwrap();
    let ImportRequest::Provider(p) = req else {
        panic!("應該是 provider");
    };
    assert_eq!(p.app, "claude");
    assert_eq!(p.name, "My Provider");
    assert_eq!(p.endpoints, vec!["https://api.example.com"]);
    assert_eq!(p.api_key, "sk-abcdef123456789");
    let pv = preview(&ImportRequest::Provider(p));
    assert!(
        !pv.fields.iter().any(|(_, v)| v.contains("abcdef123456789")),
        "預覽不可以出現完整金鑰：{pv:?}"
    );
    assert!(pv.warnings.iter().any(|w| w.contains("API Key")));
}

#[test]
fn provider_accepts_multiple_endpoints_and_flags_missing_key() {
    let url = ours(
        "v1/import?resource=provider&app=codex&name=two\
         &endpoint=https%3A%2F%2Fa.example.com,https%3A%2F%2Fb.example.com",
    );
    let ImportRequest::Provider(p) = parse(&url).unwrap() else {
        panic!()
    };
    assert_eq!(p.endpoints.len(), 2, "逗號分隔的多個網址");
    let pv = preview(&ImportRequest::Provider(p));
    assert!(
        !pv.warnings.iter().any(|w| w.contains("沒有帶上游網址")),
        "有給 endpoint 就不該警告：{pv:?}"
    );
    assert!(
        pv.fields
            .iter()
            .any(|(k, v)| k == "API Key" && v == "（空）"),
        "這條連結沒帶金鑰 → 欄位要顯示（空）：{pv:?}"
    );
}

/// 手冊的 prompt 範例（content 是 Base64 的中文提示詞）。
#[test]
fn parses_prompt_link_with_base64_content() {
    let content = "# 角色\n你是一個專業的代碼審查專家";
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let url = ours(&format!(
        "v1/import?resource=prompt&app=claude&name=%E4%BB%A3%E7%A2%BC%E5%AF%A9%E6%9F%A5&content={}",
        encoded.replace('+', "%2B").replace('=', "%3D")
    ));
    let req = parse(&url).unwrap();
    let ImportRequest::Prompt(p) = req else {
        panic!("應該是 prompt");
    };
    assert_eq!(p.name, "代碼審查");
    assert_eq!(p.content, content);
    assert!(!p.enabled, "沒寫 enabled 時預設不啟用（cc-switch 也是）");
    let pv = preview(&ImportRequest::Prompt(p));
    assert!(pv.fields.iter().any(|(k, _)| k == "內容預覽"));
}

#[test]
fn parses_skill_link() {
    let url = ours("v1/import?resource=skill&name=my-skill&repo=owner/repo&directory=skills/my-skill&branch=main");
    let req = parse(&url).unwrap();
    let ImportRequest::Skill(s) = req else {
        panic!("應該是 skill");
    };
    assert_eq!(s.repo, "owner/repo");
    assert_eq!(s.directory, "skills/my-skill");
    assert_eq!(s.branch, "main");
    assert_eq!(s.name, "my-skill");
    // 沒有 name 時，從 directory 推
    let url2 = ours("v1/import?resource=skill&repo=owner/repo&directory=a/b/c");
    let ImportRequest::Skill(s2) = parse(&url2).unwrap() else {
        panic!()
    };
    assert_eq!(s2.name, "c");
}

#[test]
fn rejects_bad_links_with_readable_reasons() {
    // 不是我們的 scheme
    assert!(parse("ccswitch://v1/import?resource=mcp").unwrap_err().contains("tokengateway://"));
    // 路徑不對
    assert!(parse(&ours("v2/export?resource=mcp")).unwrap_err().contains("不認得這個路徑"));
    // 缺 resource / 不支援的 resource
    assert!(parse(&ours("v1/import?app=claude")).unwrap_err().contains("缺少 resource"));
    assert!(parse(&ours("v1/import?resource=theme")).unwrap_err().contains("不支援的 resource"));
    // 不受管的工具
    let e = parse(&ours("v1/import?resource=provider&app=gemini&name=x")).unwrap_err();
    assert!(e.contains("不支援的工具"), "{e}");
    // 缺必填
    assert!(parse(&ours("v1/import?resource=provider&app=claude")).unwrap_err().contains("name"));
    assert!(parse(&ours("v1/import?resource=prompt&app=claude&name=x")).unwrap_err().contains("content"));
    assert!(parse(&ours("v1/import?resource=mcp&config=e30%3D")).unwrap_err().contains("apps"));
    // 壞掉的 base64 / JSON
    let bad = ours("v1/import?resource=prompt&app=claude&name=x&content=!!!");
    assert!(parse(&bad).unwrap_err().contains("Base64"));
    let bad_json = ours(&format!(
        "v1/import?resource=mcp&apps=claude&config={}",
        base64_of("not json")
    ));
    assert!(parse(&bad_json).unwrap_err().contains("合法 JSON"));
    // config 沒有 mcpServers
    let no_servers = ours(&format!(
        "v1/import?resource=mcp&apps=claude&config={}",
        base64_of("{\"x\":1}")
    ));
    assert!(parse(&no_servers).unwrap_err().contains("mcpServers"));
    // endpoint 不是 http(s)
    let bad_ep = ours("v1/import?resource=provider&app=codex&name=x&endpoint=ftp%3A%2F%2Fy");
    assert!(parse(&bad_ep).unwrap_err().contains("http(s)"));
}

fn base64_of(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

#[test]
fn mcp_entry_shapes_both_work() {
    let cfg = base64_of(
        r#"{"mcpServers":{"local":{"command":"npx","args":["-y","pkg"],"env":{"K":"v"}},
            "remote":{"type":"http","url":"https://x/mcp","headers":{"A":"B"}}}}"#,
    );
    let url = ours(&format!(
        "v1/import?resource=mcp&apps=claude,opencode&config={}",
        cfg
    ));
    let ImportRequest::Mcp(m) = parse(&url).unwrap() else {
        panic!()
    };
    assert_eq!(m.servers.len(), 2);
    let local = &m.servers.iter().find(|(id, _)| id == "local").unwrap().1;
    assert_eq!(local.transport, "stdio");
    assert_eq!(local.env[0].name, "K");
    let remote = &m.servers.iter().find(|(id, _)| id == "remote").unwrap().1;
    assert_eq!(remote.transport, "http");
    assert_eq!(remote.url, "https://x/mcp");
    assert_eq!(remote.headers[0].value, "B");
    assert_eq!(m.apps, vec!["claude", "opencode"]);
}

#[test]
fn mask_keeps_only_ends() {
    assert_eq!(mask(""), "（空）");
    assert_eq!(mask("short"), "•••••");
    let m = mask("sk-1234567890abcdef");
    assert!(m.starts_with("sk-123"), "{m}");
    assert!(m.ends_with("cdef"), "{m}");
    assert!(!m.contains("4567890abc"), "{m}");
}
