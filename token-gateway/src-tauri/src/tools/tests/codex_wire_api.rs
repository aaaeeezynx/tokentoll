//! Codex `wire_api` 的判定與回歸測試（原 `codex.rs` 的 B4／§5.3 群組）。
//!
//! **2026-09-28 定案後改寫**：`wire_api` 不再是一個「依上游能力判斷」的
//! 函式，而是單一常數 [`CODEX_WIRE_API`]。事故始末見 `docs/TESTING.md` §0.6。
//!
//! 舊的三條測試（URL 推定、宣告優先、未宣告回退）測的是**已被刪除的機制**，
//! 一併移除。`api_format` 本身仍然存在，但只剩「來源列表顯示」與「網關端
//! 轉譯」用途，不再參與協議決策 —— 這正是故障的根因，所以用型別把它拿掉：
//! `CodexAuth` 已經沒有那個欄位，呼叫端想傳也傳不進來。

use super::*;

/// 讀出所有 provider 段的 `wire_api`（缺值記為 `<缺失>`）。
fn wire_apis(out: &str) -> Vec<(String, String)> {
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
}

/// **核心不變式**：不管 `base_url` 是什麼，寫進去的 `wire_api` 一律是
/// [`CODEX_WIRE_API`]。
///
/// 這條測試的價值是「**不可能漂移**」：以前寫入端與預覽端各算一次同樣的
/// 運算式，於是出現「寫入端寫 `responses`、預覽端顯示 `chat`」這種自相
/// 矛盾。現在兩邊共用同一個常數，這條過就代表兩邊一定一致。
#[test]
fn codex_apply_always_writes_the_shared_wire_api() {
    assert_eq!(CODEX_WIRE_API, "responses", "常數本身就是定義");

    // 網關 URL（生產唯一會出現的值）→ 共用常數
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
    for (name, w) in wire_apis(&out) {
        assert_eq!(w, CODEX_WIRE_API, "[{name}] 應為共用常數：{out}");
    }

    // **連第三方 URL 也一樣。** 這看起來像退步（B4 當初就是在修這個），但：
    // (1) 直連分支已於 2026-09-28 移除，`plan_switch`／`apply_switch` 都會把
    //     `base_url` 強制改寫成網關，所以第三方 URL 根本不會出現在寫入路徑；
    // (2) Codex **已不接受 `chat`**，寫它等於讓 Codex 把整份設定判為非法。
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
        },
    )
    .unwrap();
    let got = wire_apis(&out);
    assert!(!got.is_empty(), "應產出 provider 段：{out}");
    for (name, w) in &got {
        assert_eq!(
            w, CODEX_WIRE_API,
            "即使 base_url 是第三方，[{name}] 仍應是 responses：{out}"
        );
        assert_ne!(w, "chat", "Codex 已不接受 chat：{out}");
    }
    assert!(
        got.iter().any(|(n, _)| n == "oldalias"),
        "別名段也必須存在：{got:?}"
    );
}

/// **回歸測試（2026-09-28 真實故障）**：真實的接管路徑長這樣 —— `base_url`
/// 是**本網關**，而來源宣告的是 `openai-chat`（本機 7 個來源的 `api_format`
/// **全部**如此）。
///
/// 這種組合以前會被寫成 `wire_api = "chat"`，而 Codex 讀到就把整份 config
/// 判為「Invalid configuration; using defaults」，於是回頭找 ChatGPT 登入，
/// 使用者看到的錯誤是 **「Unable to log in」** —— 完全看不出真正起因。
///
/// 現在宣告**完全不參與**協議決策（型別上就傳不進來），所以不可能再寫出
/// `chat`。這條測試同時守住「別把宣告接回來」。
#[test]
fn codex_apply_ignores_declared_chat_on_gateway() {
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
        },
    )
    .unwrap();
    let got = wire_apis(&out);
    assert!(!got.is_empty(), "應產出 provider 段：{out}");
    for (name, w) in &got {
        assert_eq!(
            w, "responses",
            "指向網關時 [{name}] 必須是 responses：{out}"
        );
    }
    assert!(
        got.iter().any(|(n, _)| n == "oldalias"),
        "別名段也必須存在：{got:?}"
    );
}

/// **預覽不得騙人**：`plan_switch` 顯示的 `wire_api` 必須與實際寫入的值一致。
///
/// 這兩處原本**各自算一次**同樣的運算式；修 `gateway_section` 時很容易漏掉
/// `plan_switch`（第一版就漏了），結果預覽顯示 `chat`、實際寫 `responses`
/// —— 而 `plan_switch` 的註解自己寫著「預覽必須顯示實際會寫入的值，
/// 否則預覽會騙人」。現在兩邊共用常數，這條守住的是「別再各自算」。
#[test]
fn plan_preview_wire_api_matches_what_is_written() {
    let req = |fmt: Option<&str>| SwitchRequest {
        app: "codex".into(),
        // 故意給第三方 URL ＋ 宣告 openai-chat：兩者都不該影響結果。
        base_url: "https://integrate.api.nvidia.com/v1".into(),
        api_key: "k".into(),
        model: "m".into(),
        provider_id: Some(15),
        provider_format: fmt.map(|s| s.to_string()),
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: true,
        key_id: None,
        claude_map: None,
    };

    for fmt in [Some("openai-chat"), None] {
        let plan = plan_switch(&req(fmt), 15722).unwrap();
        let line = plan
            .edits
            .iter()
            .find(|e| e.contains("wire_api"))
            .unwrap_or_else(|| panic!("預覽應提到 wire_api：{:?}", plan.edits));
        assert!(
            line.contains(&format!("wire_api = {}", CODEX_WIRE_API)),
            "預覽必須顯示實際會寫入的值：{line}"
        );
        assert!(
            !line.contains("wire_api = chat"),
            "預覽不得顯示 Codex 已不接受的 chat：{line}"
        );
        // base_url 仍必須被改寫成網關（另一個既有測試在管，這裡順帶確認）。
        assert!(line.contains("http://127.0.0.1:15722/v1"), "{line}");
    }
}
