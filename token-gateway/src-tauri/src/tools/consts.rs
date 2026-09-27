//! 常數與應用清單
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。



pub const GATEWAY_HOST: &str = "127.0.0.1";

/// 各工具引用本地 Key 的統一環境變量名。
pub const GATEWAY_ENV_KEY: &str = "TOKEN_GATEWAY_KEY";

/// 我們在各工具配置裡創建的渠道 ID。
pub const GATEWAY_PROVIDER_ID: &str = "tokengateway";

/// Codex 共享段名（cc-switch 同款）：會話歷史按段名引用供應商，段名必須
/// 永遠存在且指向網關，舊會話才不會因段被刪而「Model provider not found」。
pub const CODEX_SHARED_PROVIDER_ID: &str = "custom";

/// Hermes 自定義渠道按其自有約定讀取的 Key 名：`HERMES_CUSTOM_<PROVIDER>_API_KEY`。
pub const HERMES_CUSTOM_ENV_KEY: &str = "HERMES_CUSTOM_TOKENGATEWAY_API_KEY";

pub const BACKUP_KEEP: usize = 10;


/// (app_id, 顯示名稱)
pub const APPS: [(&str, &str); 7] = [
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("opencode", "OpenCode"),
    ("hermes", "Hermes Agent"),
    ("dsh", "DeepSeek Harness"),
    ("cursor", "Cursor"),
    ("antigravity", "Antigravity"),
];


/// 歷史別名上限（threads 去重後的 provider 名，防止失控膨脹）。
pub(crate) const CODEX_ALIAS_CAP: usize = 32;

/// provider 段內聯 models 上限（桌面版選擇器顯示用）。
pub(crate) const CODEX_INLINE_MODELS_CAP: usize = 200;


#[cfg(test)]
mod tests {
    use super::*;

    /// `APPS` 的顯示名是**使用者看得到的字**，而且前端 `logos.tsx` 的
    /// `APP_META` 必須與它一致。
    ///
    /// 這條測試的由來：使用者回報「本機工具應該是顯示 DSH 或 DeepSeek Harness，
    /// 而不是 DeepSeek」。查證後發現後端早就寫 `"DeepSeek Harness"`，是前端
    /// 寫成 `"DeepSeek"` —— 兩份清單各寫各的，遲早會再漂移。
    ///
    /// 這裡釘住後端；前端由 `scripts/check_app_labels.py` 比對。
    #[test]
    fn apps_display_names_are_stable() {
        let got: Vec<(&str, &str)> = APPS.to_vec();
        assert_eq!(
            got,
            vec![
                ("claude", "Claude Code"),
                ("codex", "Codex"),
                ("opencode", "OpenCode"),
                ("hermes", "Hermes Agent"),
                ("dsh", "DeepSeek Harness"),
                ("cursor", "Cursor"),
                ("antigravity", "Antigravity"),
            ],
            "APPS 的顯示名異動時，請同步更新前端 logos.tsx 的 APP_META"
        );
    }

    /// `dsh` 的顯示名必須是完整的產品名，不可簡寫成 `DeepSeek`。
    ///
    /// 這一格指的是**本機工具**（DeepSeek Harness CLI），不是模型或廠商；
    /// 寫成 `DeepSeek` 會讓使用者以為那是模型名稱。
    #[test]
    fn dsh_label_is_full_product_name() {
        let label = APPS
            .iter()
            .find(|(a, _)| *a == "dsh")
            .map(|(_, l)| *l)
            .expect("APPS 必須包含 dsh");
        assert_eq!(label, "DeepSeek Harness");
        assert_ne!(label, "DeepSeek", "不可簡寫成廠商名");
    }

    /// app id 不可重複，且每個都是非空字串。
    #[test]
    fn app_ids_are_unique_and_non_empty() {
        let mut ids: Vec<&str> = APPS.iter().map(|(a, _)| *a).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "app id 有重複");
        for (a, l) in APPS {
            assert!(!a.is_empty(), "app id 不可為空");
            assert!(!l.is_empty(), "顯示名不可為空：{a}");
        }
    }
}
