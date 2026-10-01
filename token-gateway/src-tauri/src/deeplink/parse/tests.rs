//! `deeplink::parse` 的測試。
//!
//! provider 的案例沿用 cc-switch 手冊的形狀（URL-encoded 的名稱與 endpoint），
//! 這樣「我們的解析器讀得懂那種連結」就是有出處的驗證，而不是自己編的。
//!
//! `mcp` / `prompt` / `skill` 三種資源連同功能已於 2026-10-02 移除，
//! 這裡改為**反向斷言**：舊連結要拿到可讀的拒絕原因，而不是靜默失敗。

use super::*;

/// 我們的 scheme。
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

/// 手冊的 provider 範例（URL-encoded 的名稱與 endpoint）。
#[test]
fn parses_provider_link_with_encoded_values() {
    let url = ours(
        "v1/import?resource=provider&app=claude&name=My%20Provider\
         &endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-abcdef123456789",
    );
    let req = parse(&url).unwrap();
    // 目前 ImportRequest 只有一個變體，這裡直接解構（不再需要 else）
    let ImportRequest::Provider(p) = req;
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
    let ImportRequest::Provider(p) = parse(&url).unwrap();
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

/// 已移除的三種資源：**明確拒絕**，而且原因要看得懂（2026-10-02 移除 MCP／提示詞／技能）。
#[test]
fn removed_resources_are_rejected_with_a_readable_reason() {
    for r in ["mcp", "prompt", "skill"] {
        let url = ours(&format!("v1/import?resource={r}&app=claude&name=x"));
        let e = parse(&url).unwrap_err();
        assert!(e.contains("不支援的 resource"), "{r} → {e}");
        assert!(e.contains(r), "原因要指出是哪一種資源：{r} → {e}");
        assert!(e.contains("只支援 provider"), "要說清楚現在支援什麼：{r} → {e}");
    }
}

#[test]
fn rejects_bad_links_with_readable_reasons() {
    // 不是我們的 scheme
    assert!(parse("ccswitch://v1/import?resource=provider").unwrap_err().contains("tokengateway://"));
    // 路徑不對
    assert!(parse(&ours("v2/export?resource=provider")).unwrap_err().contains("不認得這個路徑"));
    // 缺 resource / 不支援的 resource
    assert!(parse(&ours("v1/import?app=claude")).unwrap_err().contains("缺少 resource"));
    assert!(parse(&ours("v1/import?resource=theme")).unwrap_err().contains("不支援的 resource"));
    // 不受管的工具
    let e = parse(&ours("v1/import?resource=provider&app=gemini&name=x")).unwrap_err();
    assert!(e.contains("不支援的工具"), "{e}");
    // 缺必填
    assert!(parse(&ours("v1/import?resource=provider&app=claude")).unwrap_err().contains("name"));
    // 沒有 endpoint 時**解析會過**（端點可留空，讓使用者匯入後自己填）；
    // 「完全沒有 endpoint 就拒絕建立」是 apply 層的規則，見 apply.rs 的測試。
    assert!(parse(&ours("v1/import?resource=provider&app=claude&name=x")).is_ok());
    // endpoint 不是 http(s)
    let bad_ep = ours("v1/import?resource=provider&app=codex&name=x&endpoint=ftp%3A%2F%2Fy");
    assert!(parse(&bad_ep).unwrap_err().contains("http(s)"));
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
