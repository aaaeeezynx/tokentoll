//! 還原結果給使用者看的那一行摘要。
//!
//! 由 `backup.rs` 拆出（該檔在加上「只看生效中 provider」的死鏈檢查後達 421 行，
//! 超過本專案 400 行的自律上限）。**行為一字未改**，只是搬位置＋新增一個私有
//! helper；`restore_summary` 由 `tools.rs` 再匯出，`backup.rs` 與 `native.rs`
//! 都經 `use super::*;` 取用，呼叫端寫法不變。

use super::*;

/// 還原內容摘要（行級掃描，不依賴嚴格解析）＋死鏈警告。
pub(crate) fn restore_summary(text: &str) -> String {
    let mut mp = "未知".to_string();
    let mut model = "未知".to_string();
    for line in text.lines() {
        let t = line.trim();
        if mp == "未知" && t.starts_with("model_provider") {
            if let Some(v) = t.split('=').nth(1) {
                mp = v.trim().trim_matches(['"', '\'']).to_string();
            }
        }
        if model == "未知" && (t.starts_with("model ") || t.starts_with("model=") || t.starts_with("model\t")) {
            if let Some(v) = t.split('=').nth(1) {
                model = v.trim().trim_matches(['"', '\'']).to_string();
            }
        }
        if mp != "未知" && model != "未知" {
            break;
        }
    }
    let mut out = format!("provider={mp} model={model}");
    if text.parse::<toml_edit::DocumentMut>().is_err() {
        out.push_str("｜⚠️ 還原內容本身不是合法 TOML，Codex 可能無法載入");
    }
    // 直連第三方 https 但 wire_api=responses：多數第三方只說 Chat 協議，Codex 將 404。
    //
    // 只看**生效中**的那個 provider 段（`model_provider` 指名的那個）。原本是掃
    // 整份文本找 `https://` ＋ `wire_api = "responses"`，於是別名段（舊對話在用、
    // 但現在不是路由的那些）會讓警告誤報 —— 2026-09-28 的實例：關閉網關把來源
    // 換回 `openai` 之後，畫面卻還在警告「直連第三方…不可用請改走網關接管」。
    if active_provider_is_direct_chat(text) {
        out.push_str("｜⚠️ 該配置直連第三方 URL 但走 responses 協議（直連 Chat 上游會 404），能用才算數；不可用請改走網關接管或官方登入");
    }
    out
}


/// 生效中的 provider 是否「直連第三方 https ＋ responses 協議」。
fn active_provider_is_direct_chat(text: &str) -> bool {
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    // 沒有 `model_provider` = 用 Codex 內建的官方 provider，沒有第三方 URL 這回事。
    let Some(mp) = doc.get("model_provider").and_then(|i| i.as_str()) else {
        return false;
    };
    let Some(sec) = doc.get("model_providers").and_then(|t| t.get(mp)) else {
        return false;
    };
    let url = sec.get("base_url").and_then(|i| i.as_str()).unwrap_or("");
    let wire = sec.get("wire_api").and_then(|i| i.as_str()).unwrap_or("");
    url.starts_with("https://")
        && wire == "responses"
        && !url.contains("api.openai.com")
        && !url.contains(GATEWAY_HOST)
}
