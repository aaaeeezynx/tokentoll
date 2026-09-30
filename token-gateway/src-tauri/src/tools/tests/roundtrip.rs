//! 官方來源 <-> 指定來源來回切換的矩陣鎖。
//!
//! 由 `tools/tests/native.rs` 拆出來（該檔 2026-09-30 時 410 行 > 400）。
//! 測試內容與斷言一字未改；共用 helper 由父模組 `tools::tests` 提供。
//!
//! 使用者要求：「確保使用官方來源與指定來源互相切換都不會出現
//! Unable to log in 的錯誤」。這裡把它鎖在單元層級；實機矩陣另外跑過
//! （見 `docs/TESTING.md` §0.9.10）。

use super::*;

/// 「切回官方之後能不能真的登入官方」——2026-09-30 實測：Claude Code 在這台
/// 機器上沒有官方憑證，關閉後立刻回 `Not logged in · Please run /login`。
/// App 不能替他登入，但必須先講。
#[test]
fn official_auth_hint_reports_missing_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();

    // Claude：有憑證檔就不再提醒。
    let cdir = home.join(".claude");
    std::fs::create_dir_all(&cdir).unwrap();
    std::fs::write(cdir.join(".credentials.json"), r#"{"claudeAiOauth":{"accessToken":"x"}}"#).unwrap();
    assert!(official_auth_hint("claude", home, "{}").is_none());

    // 設定裡有 API key 也算有憑證。
    std::fs::remove_file(cdir.join(".credentials.json")).unwrap();
    assert!(official_auth_hint("claude", home, r#"{"env":{"ANTHROPIC_API_KEY":"sk-a"}}"#).is_none());

    // 兩者都沒有 → 必須提醒，而且要提到該做什麼。
    let hint = official_auth_hint("claude", home, "{}").expect("沒有憑證時應該提醒");
    assert!(hint.contains("Not logged in"), "{hint}");
    assert!(hint.contains("/login"), "{hint}");

    // Codex / OpenCode：同樣的規則，靠各自的 auth.json。
    assert!(official_auth_hint("codex", home, "{}").is_some());
    assert!(official_auth_hint("opencode", home, "{}").is_some());
    let kdir = home.join(".codex");
    std::fs::create_dir_all(&kdir).unwrap();
    std::fs::write(kdir.join("auth.json"), r#"{"OPENAI_API_KEY":"x"}"#).unwrap();
    assert!(official_auth_hint("codex", home, "{}").is_none());
    let odir = home.join(".local").join("share").join("opencode");
    std::fs::create_dir_all(&odir).unwrap();
    std::fs::write(odir.join("auth.json"), r#"{"opencode":{}}"#).unwrap();
    assert!(official_auth_hint("opencode", home, "{}").is_none());

    // 空檔案不算憑證。
    std::fs::write(kdir.join("auth.json"), "  \n").unwrap();
    assert!(official_auth_hint("codex", home, "{}").is_some());

    // 不支援的工具不亂講話。
    assert!(official_auth_hint("cursor", home, "{}").is_none());
}

/// 真實事故 §0.9.10 的整合版鎖：官方（原生）與指定來源（接管）來回切換，
/// 全程不得出現 Codex 的內建 provider 段。DB 別名刻意帶著 `openai`／`ollama`
/// ——那是「用原生來源跑過之後，Codex 自己對話紀錄裡就會有的名字」，
/// 也正是當初把禁用段帶進設定檔的來源。
#[test]
fn codex_official_and_specified_sources_swap_repeatedly() {
    let gw = "http://127.0.0.1:15722/v1";
    let db: Vec<String> = ["openai", "ollama", "tokencustom"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut text = NIM_CONFIG.to_string();
    for round in 0..2 {
        // → 指定來源（接管）
        let aliases = codex_alias_ids(&text, &db, gw);
        text = codex_apply(
            &text,
            gw,
            "deepseek-v4.1-flash",
            None,
            None,
            &aliases,
            &[],
            CodexAuth::default(),
        )
        .unwrap();
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(
            doc["model_provider"].as_str(),
            Some(CODEX_SHARED_PROVIDER_ID),
            "第 {round} 輪"
        );
        let mp = doc.get("model_providers").and_then(|m| m.as_table()).unwrap();
        for bad in CODEX_RESERVED_PROVIDER_IDS {
            assert!(
                mp.get(bad).is_none(),
                "第 {round} 輪接管冒出內建段 {bad}：{text}"
            );
        }

        // → 官方來源（還原＋原生化）
        let nat = to_native("codex", &text, 15722);
        assert!(nat.warning.is_none(), "{:?}", nat.warning);
        text = nat.text;
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(
            doc["model_provider"].as_str(),
            Some("openai"),
            "第 {round} 輪"
        );
        let mp = doc.get("model_providers").and_then(|m| m.as_table());
        for bad in CODEX_RESERVED_PROVIDER_IDS {
            assert!(
                mp.map(|m| m.get(bad).is_none()).unwrap_or(true),
                "第 {round} 輪還原冒出內建段 {bad}：{text}"
            );
        }
    }
}
