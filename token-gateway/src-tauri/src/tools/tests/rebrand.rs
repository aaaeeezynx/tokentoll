//! 品牌改名（Token Gateway → Token Toll）的相容性回歸測試。
//!
//! 2026-10-04 由 Token Gateway 更名為 Token Toll。渠道 ID、環境變數名、匯出格式
//! 都已經寫進使用者的系統與工具設定，所以**寫入用新名、讀取新舊都認**。
//! 這裡釘住「舊名仍然可用」這件事，避免日後有人順手把相容分支清掉。
//!
//! 從 `tools/tests/codex.rs` 抽出來（該檔在改名後超過 400 行上限）。

use super::*;
use crate::providers_io::{EXPORT_KIND, EXPORT_KIND_LEGACY, EXPORT_KINDS};

/// 接管時，新舊渠道段名**都要**建立並指向網關。
///
/// 舊會話歷史是按 provider 段名引用供應商的，段名消失會讓 Codex 報
/// 「Model provider not found」—— 所以舊段名不能只是「不刪」，必須繼續維護。
#[test]
fn takeover_creates_both_new_and_legacy_provider_sections() {
    let out = codex_apply(
        "",
        "http://127.0.0.1:15722/v1",
        "m",
        None,
        None,
        &[],
        &[],
        CodexAuth::default(),
    )
    .unwrap();
    // 共享段（cc-switch 同款）＋ 新舊兩個渠道段
    for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID, "tokengateway"] {
        assert!(
            out.contains(&format!("[model_providers.{id}]")),
            "缺少 {id} 段：{out}"
        );
    }
    let v: toml_edit::DocumentMut = out.parse().unwrap();
    for id in [GATEWAY_PROVIDER_ID, "tokengateway"] {
        assert_eq!(
            v["model_providers"][id]["base_url"].as_str(),
            Some("http://127.0.0.1:15722/v1"),
            "{id} 段必須指向網關"
        );
    }
    // 寫入的是新名
    assert!(out.contains("env_key = \"TOKEN_TOLL_KEY\""), "{out}");
}

/// 舊的環境變數名仍然被體檢認可（使用者不必立刻改系統環境變數）。
#[test]
fn doctor_accepts_env_key_under_either_name() {
    // 兩種名字都應該在「讀取候選」清單裡。
    assert!(GATEWAY_ENV_KEYS.contains(&GATEWAY_ENV_KEY));
    assert!(GATEWAY_ENV_KEYS.contains(&GATEWAY_ENV_KEY_LEGACY));
    assert_eq!(GATEWAY_ENV_KEY, "TOKEN_TOLL_KEY");
    assert_eq!(GATEWAY_ENV_KEY_LEGACY, "TOKEN_GATEWAY_KEY");
}

/// 匯入端接受新舊兩種 `kind`，舊的匯出檔不會因為改名而失效。
#[test]
fn import_accepts_legacy_export_kind() {
    assert!(EXPORT_KINDS.contains(&EXPORT_KIND));
    assert!(EXPORT_KINDS.contains(&EXPORT_KIND_LEGACY));
    assert_eq!(EXPORT_KIND_LEGACY, "token-gateway/providers");
}

/// 還原時，「釘住的網關模型前綴」新舊都要認。
#[test]
fn native_restore_drops_pinned_model_under_either_prefix() {
    for prefix in [GATEWAY_PROVIDER_ID, GATEWAY_PROVIDER_ID_LEGACY] {
        let text = format!(
            "{{\"provider\": {{}}, \"model\": \"{prefix}/some-model\"}}"
        );
        let out = to_native("opencode", &text, 15722);
        let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
        assert!(
            v.get("model").is_none(),
            "prefix={prefix} 的釘住模型必須被移除：{}",
            out.text
        );
    }
    // 使用者自己的 model 不准動
    let mine = "{\"provider\": {}, \"model\": \"anthropic/claude-sonnet-4-6\"}";
    let out = to_native("opencode", mine, 15722);
    let v: serde_json::Value = serde_json::from_str(&out.text).unwrap();
    assert_eq!(v["model"], "anthropic/claude-sonnet-4-6");
}
