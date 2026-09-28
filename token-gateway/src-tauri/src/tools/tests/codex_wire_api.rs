//! Codex `wire_api` 的判定與回歸測試（原 `codex.rs` 的 B4／§5.3 群組）。
//!
//! 由 `tools.rs` 的測試模組拆出而供。測試內容與新言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;

/// B4 迴歸測試：`wire_api` 不可無差別寫 `responses`。
///
/// **設定檔裡是第三方 URL**（早先的直連模式留下的、或手改的）時，多數第三方
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

/// **回歸測試（2026-09-28 真實故障）**：真實的接管路徑長這樣 ——
/// `base_url` 是**本網關**，而 provider 宣告的是 `openai-chat`
/// （本機 7 個來源的 `api_format` **全部**如此）。
///
/// 這種組合以前會被寫成 `wire_api = "chat"`，而 **Codex 已不再接受 `chat`**：
/// 它把整份 config 判為「Invalid configuration; using defaults」，於是回頭
/// 找 ChatGPT 登入，使用者看到的錯誤是 **「Unable to log in」**
/// —— 完全看不出真正的起因。指向網關時必須一律 `responses`。
#[test]
fn codex_apply_forces_responses_on_gateway_even_if_declared_chat() {
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

    // 決策函式：指向網關／官方 OpenAI 時，宣告不得推翻。
    assert_eq!(
        codex_wire_api_for("http://127.0.0.1:15722/v1", Some("chat")),
        "responses"
    );
    assert_eq!(
        codex_wire_api_for("http://127.0.0.1:15722/v1", None),
        "responses"
    );
    assert_eq!(
        codex_wire_api_for("https://api.openai.com/v1", Some("chat")),
        "responses"
    );
    // 真正的第三方：宣告才有作用（此分支在生產路徑已不可達 —— 直連分支
    // 於 2026-09-28 移除；保留是因為讀取既有設定檔時仍需判讀）。
    assert_eq!(
        codex_wire_api_for("https://api.example.com/v1", Some("responses")),
        "responses"
    );
    assert_eq!(
        codex_wire_api_for("https://api.example.com/v1", None),
        "chat"
    );

    // 端到端：完全比照 `switch.rs` 的真實呼叫（網關 URL ＋ 宣告 openai-chat）。
    let declared = codex_wire_api_declared(Some("openai-chat"));
    assert_eq!(declared, Some("chat"), "前提：宣告確實是 chat");
    let out = codex_apply(
        "model = \"m\"\n",
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &["oldalias".to_string()],
        &[],
        CodexAuth {
            direct_key: Some("nv-key"),
            wire_api: declared,
        },
    )
    .unwrap();
    let got = get(&out);
    assert!(!got.is_empty(), "應產出 provider 段：{out}");
    for (name, w) in &got {
        assert_eq!(
            w, "responses",
            "指向網關時 [{name}] 必須是 responses（宣告 openai-chat 不得推翻）：{out}"
        );
    }
    assert!(
        got.iter().any(|(n, _)| n == "oldalias"),
        "別名段也必須存在：{got:?}"
    );
}
