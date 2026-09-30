//! 「關閉網關」的最後一步：把工具還原成它**自己的第一方來源**。
//!
//! ## 為什麼不能只靠還原備份
//!
//! `restore_backup_port` 是**忠實**還原 —— 它把「接管前的那份備份」逐字寫回
//! 設定檔。但那份備份可能是很久以前的快照：2026-09-28 的實例是基線停在
//! 09-26（`model_provider = "custom"` → NIM、`model = "moonshotai/kimi-k3"`、
//! `model_catalog_json` → `codex-15.json`），使用者關掉網關後 Codex 因此卡在
//! 一個他早就不用的第三方來源，模型清單裡也只有 NIM 的模型。
//!
//! 他期待的是「Codex 回到 GPT、Claude Code 回到 Claude」。所以關閉流程拆成兩步：
//!
//! 1. `restore_backup_port` —— 忠實還原，保住使用者自己的設定，
//!    以及**別名 provider 段**（舊對話的 rollout 記錄了 provider 名，
//!    段被刪掉那些對話就開不起來；見 `switch.rs` 的 `codex_alias_ids`）。
//! 2. 本模組 —— 只把「來源」相關的鍵換回第一方。
//!
//! ## 各工具的「原生」定義
//!
//! | 工具 | 做什麼 | 為什麼 |
//! |---|---|---|
//! | codex | `model_provider = "openai"`、`model = "gpt-5.6-luna"`、移除 `model_catalog_json` | 移除 App 產生的目錄後，Codex 才會用內建的 GPT 模型清單 |
//! | claude | 移除任何不是 Anthropic 官方的 `ANTHROPIC_BASE_URL`（連帶 `ANTHROPIC_AUTH_TOKEN`）；移除非 `claude-*` 的模型對映 | 讓 Claude Code 回到官方端點與 Claude 模型 |
//! | opencode | 移除指向網關的 `provider.*.options.baseURL`／`apiKey` | 回到 OpenCode 自己的登入 |
//!
//! **刻意保留**：使用者自己寫的鍵、`.codex` 的別名 provider 段、`.claude` 的
//! `theme`、opencode 的其他 provider 段。關閉網關不該順手刪掉使用者的東西。
//!
//! ⚠️ **Claude Code 的端點是例外**：使用者 2026-09-28 明確選定「連端點一起推回
//! Anthropic 官方」，所以他自己原本的 router（例如 `opencode.ai/zen/go`）也會被
//! 移除。代價是他必須先 `claude` 登入一次 —— 在那之前 Claude Code 不能跑。
//!
//! ## 為什麼不改 `restore_backup_to_inner`
//!
//! 那支的行為被測試釘住（「還原 = 逐字寫回備份」，見 `tests/restore.rs`），
//! 那個性質本身是安全網。本模組是**額外**的一步，由命令層
//! [`restore_native_port`] 串起來。

use super::*;
use toml_edit::{value, DocumentMut};

/// Codex 的第一方 provider id（Codex 內建，不需要 `[model_providers]` 段）。
pub const CODEX_NATIVE_PROVIDER: &str = "openai";

/// Claude Code 的第一方端點。`ANTHROPIC_BASE_URL` 指向這裡以外的一律移除。
pub const ANTHROPIC_HOST: &str = "api.anthropic.com";

/// Codex 還原後預設使用的模型。
///
/// 選 `gpt-5.6-luna` 是因為使用者自己的舊對話就是用這個模型（Codex 的日誌裡
/// 出現過 `Unknown model gpt-5.6-luna` 的警告 —— 當時 config 的目錄是 NIM 的，
/// 所以查不到）。使用者於 2026-09-28 明確選定它。
///
/// **為什麼是「一律覆寫」而不是「已經是 GPT 系列就別動」**（2026-09-29 查證）：
/// 還原當下 config 裡殘留的模型名可能**根本不是 OpenAI 的模型**，而是別的來源
/// 目錄帶進來的。實例：使用者那份原生設定寫著 `model = "gpt-6-luna"`，但
///
/// - Codex 自己的內建清單（乾淨 `CODEX_HOME`）只有 `gpt-6-astra` 這個 gpt-6，
///   `luna` 只有 `gpt-5.6-luna`；查不到 `gpt-6-luna`；
/// - App 的 `provider_models` 裡 `gpt-6-luna` 只掛在 provider 22（opencode-go），
///   `catalogs/codex-22.json` 是唯一含它的目錄檔。
///
/// 也就是說 `gpt-6-luna` 是 **opencode-go 的模型名**。原生還原的目的就是離開那些
/// 第三方來源，留著它們的模型名只會讓 Codex 拿到一個查不到的模型（正是上面那個
/// `Unknown model` 警告的成因），所以一律覆寫成本常數是正確行為 ——
/// 使用者於 2026-09-29 確認。
pub const CODEX_NATIVE_MODEL: &str = "gpt-5.6-luna";

/// 原生化的結果。`text` 一律可直接寫回檔案（無法轉換時等於輸入）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativeOutcome {
    pub text: String,
    /// 人類可讀的變更摘要；沒有變更時為空。
    pub changes: Vec<String>,
    /// **無法完成**轉換的原因（此時 `text` 等於輸入）。畫面會加 ⚠️。
    pub warning: Option<String>,
    /// **沒有東西要改**這種正常結果的說明。畫面**不加** ⚠️ ——
    /// 「這個工具本來就在原生來源」不是異常，對健康的設定示警會讓人以為壞了
    /// （2026-09-28 實機就踩到：關閉一個已經乾淨的 OpenCode，畫面出現
    /// 「⚠️ 設定裡沒有網關痕跡，維持原樣」）。
    pub note: Option<String>,
}


/// 把工具設定文本轉成「原生來源」。純函數，不碰 IO。
pub fn to_native(app: &str, text: &str, port: u16) -> NativeOutcome {
    match app {
        "codex" => codex_native(text),
        "claude" => claude_native(text, port),
        "opencode" => opencode_native(text, port),
        other => NativeOutcome {
            text: text.to_string(),
            warning: Some(format!("{other} 不支援原生還原")),
            ..Default::default()
        },
    }
}


fn codex_native(text: &str) -> NativeOutcome {
    let mut out = NativeOutcome {
        text: text.to_string(),
        ..Default::default()
    };
    let mut doc: DocumentMut = match text.parse() {
        Ok(d) => d,
        Err(e) => {
            out.warning = Some(format!("config.toml 不是合法 TOML，略過原生轉換（{e}）"));
            return out;
        }
    };
    if doc.get("model_provider").and_then(|i| i.as_str()) != Some(CODEX_NATIVE_PROVIDER) {
        doc["model_provider"] = value(CODEX_NATIVE_PROVIDER);
        out.changes
            .push(format!("model_provider → {CODEX_NATIVE_PROVIDER}"));
    }
    if doc.get("model").and_then(|i| i.as_str()) != Some(CODEX_NATIVE_MODEL) {
        doc["model"] = value(CODEX_NATIVE_MODEL);
        out.changes.push(format!("model → {CODEX_NATIVE_MODEL}"));
    }
    if doc.remove("model_catalog_json").is_some() {
        out.changes
            .push("移除 model_catalog_json（App 產生的模型目錄；移除後才看得到 GPT 模型）".into());
    }
    // Codex 內建 id（`openai` 等）不可在設定檔裡定義，留著會讓整份 config.toml
    // 載入失敗、表現成「無法登入」。還原是最適合清掉它的時機（回到官方時本來
    // 就不需要任何 provider 段）。
    if let Some(mp) = doc.get_mut("model_providers").and_then(|m| m.as_table_mut()) {
        let mut removed: Vec<&str> = vec![];
        for id in super::codex::CODEX_RESERVED_PROVIDER_IDS {
            if mp.remove(id).is_some() {
                removed.push(id);
            }
        }
        if !removed.is_empty() {
            out.changes.push(format!(
                "移除 Codex 內建 provider 段 {}（Codex 不允許覆寫內建 id；留著會讓整份設定載入失敗）",
                removed.join("、")
            ));
        }
    }
    out.text = doc.to_string();
    out
}


fn claude_native(text: &str, port: u16) -> NativeOutcome {
    let mut out = NativeOutcome {
        text: text.to_string(),
        ..Default::default()
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(text) else {
        out.warning = Some("settings.json 不是合法 JSON，略過原生轉換".into());
        return out;
    };
    let Some(env) = root.get_mut("env").and_then(|e| e.as_object_mut()) else {
        out.warning = Some("找不到 env 區塊，略過原生轉換".into());
        return out;
    };

    // 端點：任何「不是 Anthropic 官方」的覆寫都移除 —— 那一行就是「Claude Code
    // 被指到別的地方」的全部內容（網關、或使用者自己的 router 都一樣）。
    //
    // 2026-09-28 使用者明確選定連端點一起推回官方。代價是他必須先登入一次
    // （這台機器上沒有 `.claude/.credentials.json`），在那之前 Claude Code 不能跑。
    let redirect = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .map(|u| {
            reqwest::Url::parse(u)
                .map(|p| p.host_str() != Some(ANTHROPIC_HOST))
                .unwrap_or(true)
        })
        .unwrap_or(false);
    if redirect {
        let old = env
            .remove("ANTHROPIC_BASE_URL")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        if is_gateway(&old, port) {
            out.changes
                .push("移除 ANTHROPIC_BASE_URL（原本指向本網關）".into());
        } else {
            out.changes
                .push(format!("移除 ANTHROPIC_BASE_URL（原本是 {old}）"));
        }
        // 權杖是為了那個端點而存在的；端點拿掉，它就沒有意義了（值不寫進訊息）。
        if env.contains_key("ANTHROPIC_AUTH_TOKEN") {
            env.remove("ANTHROPIC_AUTH_TOKEN");
            out.changes
                .push("移除 ANTHROPIC_AUTH_TOKEN（那是給被移除的端點用的）".into());
        }
        // 上下文上限是為「網關那邊那個來源模型」寫的（`claude_apply` 的
        // `CLAUDE_CODE_MAX_CONTEXT_TOKENS`）；回到原生 Claude，它自己有內建模型表，
        // 留著只會把官方模型的視窗寫成錯的數字。
        if env.remove("CLAUDE_CODE_MAX_CONTEXT_TOKENS").is_some() {
            out.changes.push(
                "移除 CLAUDE_CODE_MAX_CONTEXT_TOKENS（那是給網關上的來源模型用的）".into(),
            );
        }
    }

    // 非 Claude 家族的模型對映：留著只會讓 Claude Code 去要一個不存在的模型。
    // `ANTHROPIC_DEFAULT_*_MODEL_NAME` 也在守備範圍（值通常是 `claude-opus-5`，會留著）。
    let foreign: Vec<String> = env
        .iter()
        .filter(|(k, _)| k.contains("MODEL"))
        .filter(|(_, v)| {
            v.as_str()
                .is_some_and(|s| !s.trim().is_empty() && !s.starts_with("claude-"))
        })
        .map(|(k, _)| k.clone())
        .collect();
    for k in foreign {
        let old = env
            .remove(&k)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        out.changes.push(format!("移除 {k}（{old} 不是 Claude 模型）"));
    }

    if out.changes.is_empty() {
        out.note = Some("設定裡沒有可原生化的地方，維持原樣".into());
    }
    out.text = serde_json::to_string_pretty(&root).unwrap_or_else(|_| text.to_string());
    if !out.text.ends_with('\n') {
        out.text.push('\n');
    }
    out
}


fn opencode_native(text: &str, port: u16) -> NativeOutcome {
    let mut out = NativeOutcome {
        text: text.to_string(),
        ..Default::default()
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(text) else {
        out.warning = Some("opencode.json 不是合法 JSON，略過原生轉換".into());
        return out;
    };
    let Some(providers) = root.get_mut("provider").and_then(|p| p.as_object_mut()) else {
        out.warning = Some("找不到 provider 區塊，略過原生轉換".into());
        return out;
    };
    let mut changes = Vec::new();
    for (pname, pval) in providers.iter_mut() {
        let Some(opts) = pval.get_mut("options").and_then(|o| o.as_object_mut()) else {
            continue;
        };
        let gw_url = opts
            .get("baseURL")
            .and_then(|v| v.as_str())
            .is_some_and(|u| is_gateway(u, port));
        if gw_url {
            opts.remove("baseURL");
            changes.push(format!("移除 provider.{pname}.options.baseURL（原本指向本網關）"));
        }
        // 網關自發的本地 key（`sk-local-`）只有在端點也指向網關時才確定是接管產物。
        let gw_key = gw_url
            || opts
                .get("apiKey")
                .and_then(|v| v.as_str())
                .is_some_and(|k| k.starts_with("sk-local-"));
        if gw_key && opts.remove("apiKey").is_some() {
            changes.push(format!("移除 provider.{pname}.options.apiKey（網關的本地 key）"));
        }
    }
    // 接管時釘住的頂層 `model`（`tokengateway/<模型>`）也要拿掉，否則回到原生
    // 來源後 `opencode run` 仍會指名一個已經不在設定裡的 provider。只動「指向
    // 本網關」的值：使用者自己的 `model`（例如 `anthropic/...`）不是我們的東西。
    let gw_prefix = format!("{GATEWAY_PROVIDER_ID}/");
    let pinned = root
        .get("model")
        .and_then(|m| m.as_str())
        .is_some_and(|m| m.starts_with(&gw_prefix));
    if pinned {
        root.as_object_mut().map(|o| o.remove("model"));
        changes.push("移除頂層 model（接管時釘住的網關模型）".into());
    }
    out.changes = changes;
    if out.changes.is_empty() {
        out.note = Some("設定裡沒有網關痕跡，維持原樣".into());
    }
    out.text = serde_json::to_string_pretty(&root).unwrap_or_else(|_| text.to_string());
    if !out.text.ends_with('\n') {
        out.text.push('\n');
    }
    out
}


/// 回到官方來源之後，這個工具**能不能真的登入官方**？不能就回一句警告。
///
/// 2026-09-30 實測：Claude Code 在這台機器上從來沒有官方憑證
/// （`~/.claude/.credentials.json` 不存在），所以「關閉網關」把它推回 Anthropic
/// 官方之後，它立刻回：
///
/// ```text
/// Not logged in · Please run /login
/// ```
///
/// App 沒辦法替使用者登入（那是帳號層級的事），但可以在還原的結果訊息裡
/// **先講清楚要去做什麼** —— 這樣同一句話就不再是一個看起來像 bug 的意外。
/// 這正是 Codex 沒問題的原因：`~/.codex/auth.json` 有 ChatGPT 登入，
/// 所以它回到官方之後直接可用（同日實測 exit 0）。
///
/// `settings_text` 是**原生化之後**的設定內容，用來判斷有沒有 API key 形式的憑證。
pub(crate) fn official_auth_hint(app: &str, home: &Path, settings_text: &str) -> Option<String> {
    let non_empty = |p: &Path| -> bool {
        std::fs::read_to_string(p)
            .map(|t| !t.trim().trim_matches(|c| c == '{' || c == '}').trim().is_empty())
            .unwrap_or(false)
    };
    match app {
        "claude" => {
            let has_env_key = std::env::var("ANTHROPIC_API_KEY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let has_cfg_key = settings_text.contains("ANTHROPIC_API_KEY");
            let cred = home.join(".claude").join(".credentials.json");
            if has_env_key || has_cfg_key || non_empty(&cred) {
                return None;
            }
            Some(format!(
                "這台機器找不到 Anthropic 官方登入憑證（{} 不存在、也沒有 ANTHROPIC_API_KEY）：\
                 切回官方後請先執行 `claude` 完成 /login，否則它會回「Not logged in」",
                cred.display()
            ))
        }
        "codex" => {
            let auth = home.join(".codex").join("auth.json");
            if non_empty(&auth) {
                return None;
            }
            Some(format!(
                "找不到 Codex 官方登入憑證（{}）：切回官方後請先執行 `codex login`",
                auth.display()
            ))
        }
        "opencode" => {
            let auth = home
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json");
            if non_empty(&auth) {
                return None;
            }
            Some(format!(
                "找不到 OpenCode 自己的登入憑證（{}）：沒有網關接管時它會用它自己的預設模型，\
                 請先 `opencode auth login`",
                auth.display()
            ))
        }
        _ => None,
    }
}


/// 「關閉網關」= 忠實還原 + 原生來源化。
///
/// 回傳值直接顯示在工具卡片下方，所以要講清楚「還原自哪份備份」與
/// 「換成了什麼原生來源」。
pub fn restore_native_to_port(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
) -> Result<String, String> {
    let restored = crate::tools::backup::restore_backup_to_port(app_data, app, cfg, port)?;
    let before = std::fs::read_to_string(cfg).map_err(|e| format!("讀回設定檔失敗：{e}"))?;
    let outcome = to_native(app, &before, port);
    // 只有在真的變更時才寫。這裡的寫入是安全的：`before` 就是剛剛還原進來的
    // 那份備份，內容仍完整留在 `backups/<app>/` 底下。
    if outcome.text != before {
        crate::fsutil::atomic_write(cfg, outcome.text.as_bytes()).map_err(|e| e.to_string())?;
    }
    let mut msg = match restored.find("｜provider=") {
        // 摘要要以「轉換後」的內容重算。原摘要掃描的是還原進來的那份文本，
        // 而別名 provider 段本來就會留著 —— 於是「直連第三方 URL 卻走 responses
        // 協議…不可用請改走網關接管」這種**已經不成立**的警告會被端出來，
        // 正好在使用者剛回到官方來源時嚇他一次。
        //
        // 用 `｜provider=` 定位而不是切 `｜`：摘要前面的段落本身也可能含 `｜`
        // （cc-switch 提示就是 `｜⚠️ 還原後走 cc-switch…`）。
        Some(i) => format!(
            "{}｜{}",
            &restored[..i],
            restore_summary(app, &outcome.text)
        ),
        None => restored,
    };
    if !outcome.changes.is_empty() {
        msg.push_str("｜已切回原生來源：");
        msg.push_str(&outcome.changes.join("、"));
    }
    if let Some(n) = outcome.note {
        msg.push_str(&format!("｜{n}"));
    }
    if let Some(w) = outcome.warning {
        msg.push_str(&format!("｜⚠️ {w}"));
    }
    // 官方憑證檢查：講在最後，因為那是使用者接下來要做的動作（不是轉換失敗）。
    if let Ok(home) = user_home() {
        if let Some(h) = official_auth_hint(app, &home, &outcome.text) {
            msg.push_str(&format!("｜⚠️ {h}"));
        }
    }
    Ok(msg)
}


/// 還原指定工具到「原生來源」（埠與設定檔路徑由正式路徑決定）。
pub fn restore_native_port(app_data: &Path, app: &str, port: u16) -> Result<String, String> {
    let cfg = config_path_for(app)?;
    restore_native_to_port(app_data, app, &cfg, port)
}
