//! Codex 還原時的 provider 段清理。
//!
//! 從 `tools/native.rs` 抽出來（該檔在品牌改名後超過 400 行上限）。
//! 這裡只做一件事：把「不該留在使用者設定裡的 provider 段」移除，並回報做了什麼。

use super::*;

/// 清理 `config.toml` 的 `[model_providers.*]`，回傳給使用者看的變更說明。
///
/// 兩類會被移除：
///
/// 1. **Codex 內建 id**（`openai` 等）—— 內建 id 不可在設定檔裡定義，留著會讓
///    整份 config.toml 載入失敗、表現成「無法登入」。還原是最適合清掉它的時機
///    （回到官方時本來就不需要任何 provider 段）。
/// 2. **還指向本網關的渠道段**（含品牌改名前的舊段名）—— 否則回到原生來源後
///    會留下孤兒段。
///
/// **只刪「還指向本網關」的渠道段**：使用者可能已經把同名段改成自己的直連設定
/// （例如 `[model_providers.tokengateway]` 指向 NVIDIA），那是他的東西，還原不該
/// 順手刪掉。判準是 `base_url` 是否為本機網關位址，與 `custom` 段一致。
pub(crate) fn strip_managed_provider_sections(doc: &mut toml_edit::DocumentMut) -> Vec<String> {
    let Some(mp) = doc.get_mut("model_providers").and_then(|m| m.as_table_mut()) else {
        return vec![];
    };
    let mut removed: Vec<String> = vec![];

    for id in super::codex::CODEX_RESERVED_PROVIDER_IDS {
        if mp.remove(id).is_some() {
            removed.push(id.to_string());
        }
    }

    let gateway_prefix = format!("http://{}:", GATEWAY_HOST);
    for id in super::consts::GATEWAY_PROVIDER_IDS {
        let points_at_gateway = mp
            .get(id)
            .and_then(|s| s.get("base_url"))
            .and_then(|u| u.as_str())
            .is_some_and(|u| u.starts_with(&gateway_prefix));
        if points_at_gateway && mp.remove(id).is_some() {
            removed.push(id.to_string());
        }
    }

    if !removed.is_empty() {
        return vec![format!(
            "移除 provider 段 {}（Codex 內建 id 不可覆寫；指向本網關的孤兒段也一併清掉）",
            removed.join("、")
        )];
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_of(text: &str) -> toml_edit::DocumentMut {
        text.parse().expect("合法 TOML")
    }

    #[test]
    fn removes_reserved_ids_and_gateway_sections() {
        let mut doc = doc_of(
            "model_provider = \"custom\"\n\
             [model_providers.openai]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
             [model_providers.tokentoll]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\
             [model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        );
        let changes = strip_managed_provider_sections(&mut doc);
        assert_eq!(changes.len(), 1, "{changes:?}");
        let mp = doc.get("model_providers").and_then(|m| m.as_table()).unwrap();
        assert!(mp.get("openai").is_none(), "內建 id 必須清掉");
        assert!(mp.get("tokentoll").is_none(), "本網關段必須清掉");
        assert!(mp.get("tokengateway").is_none(), "舊網關段必須清掉");
    }

    #[test]
    fn keeps_user_owned_sections_that_reuse_our_names() {
        // 使用者把舊段名改成自己的直連設定 → 不是我們的東西，不准刪。
        let mut doc = doc_of(
            "[model_providers.tokengateway]\n\
             base_url = \"https://integrate.api.nvidia.com/v1\"\n",
        );
        let changes = strip_managed_provider_sections(&mut doc);
        assert!(changes.is_empty(), "不該有變更：{changes:?}");
        let mp = doc.get("model_providers").and_then(|m| m.as_table()).unwrap();
        assert!(mp.get("tokengateway").is_some(), "使用者的段必須保留");
    }

    #[test]
    fn is_idempotent_on_missing_table() {
        let mut doc = doc_of("model = \"m\"\n");
        assert!(strip_managed_provider_sections(&mut doc).is_empty());
    }
}
