//! 網關 URL、協議推定與 Codex `config.toml` 改寫。

use super::super::*;
use super::legacy::dedupe_codex_sections;

/// 按工具拼網關 URL（M2 由前端計算後傳入；M3 代理側複用本函數做權威拼接）。
#[allow(dead_code)]
pub fn gateway_url(port: u16, app: &str) -> String {
    let base = format!("http://{GATEWAY_HOST}:{port}");
    match app {
        "claude" => base,
        _ => format!("{base}/v1"),
    }
}


pub(crate) fn codex_text_base_url(text: &str) -> Option<String> {
    let doc = text.parse::<toml_edit::DocumentMut>().ok()?;
    let provider = doc
        .get("model_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("openai");
    doc.get("model_providers")
        .and_then(|p| p.get(provider))
        .and_then(|p| p.get("base_url"))
        .and_then(|u| u.as_str())
        .map(|u| u.to_string())
}


/// 這個 `base_url` 該用哪種 `wire_api`（Codex 送出時的協議形狀）。
///
/// **為什麼不能無差別寫 `responses`**：`wire_api` 決定 Codex 用哪種協議打
/// `base_url` —— `responses` 走 `/v1/responses`、`chat` 走
/// `/v1/chat/completions`。本網關**兩種都收**並代為轉譯，所以指向網關時用
/// `responses` 沒問題（那也是 Codex 的原生形狀，能保留 reasoning 等欄位）。
/// 但**設定檔裡若出現第三方 URL**（早先的「真·直連」模式留下的、使用者手改的、
/// 或別的代理如 cc-switch 寫入的），就必須用 Chat Completions —— 硬寫
/// `responses` 會讓 Codex 打到不存在的端點而 404。
///
/// 前端那個 `base_url: via ? gatewayUrl(...) : provider.base_url` 的直連分支
/// 已於 2026-09-28 移除（`docs/SIMPLIFICATION-PLAN.md` §4.4）；這個依 URL 推定
/// 的後備仍然需要，因為還原／診斷路徑要判斷的是**檔案裡現有的** URL。
///
/// 這個問題原本**已經被發現過**（`restore_backup` 的提示文字寫著「該配置
/// 直連第三方 URL 但走 responses 協議（直連 Chat 上游會 404）」），但當時只
/// **警告、不修**。這裡改成寫入時就選對，讓那個警告不再需要出現。
pub fn codex_wire_api(base_url: &str) -> &'static str {
    let u = base_url.trim().to_ascii_lowercase();
    // 指向本網關：兩種協議都收，用 Codex 原生的 responses（保留 reasoning 等）。
    if u.contains(GATEWAY_HOST) {
        return "responses";
    }
    // 官方 OpenAI：兩種都支援，維持 responses。
    if u.contains("api.openai.com") {
        return "responses";
    }
    // 其餘第三方：Chat Completions 是唯一普遍實作的形狀。
    "chat"
}


pub(crate) fn gateway_section(
    base_url: &str,
    inline_models: &[(String, String)],
    auth: CodexAuth<'_>,
) -> toml_edit::Table {
    let mut tbl = toml_edit::Table::new();
    tbl["name"] = toml_edit::value("Token Gateway");
    tbl["base_url"] = toml_edit::value(base_url);
    match auth.direct_key {
        // 直連上游：Bearer 即上游 Key，明文寫入（用戶顯式選擇，見警告）
        Some(k) => {
            tbl["experimental_bearer_token"] = toml_edit::value(k);
        }
        None => {
            tbl["env_key"] = toml_edit::value(GATEWAY_ENV_KEY);
        }
    }
    // 協議形狀：**優先採用 provider 的明確宣告**（`api_format`），只有在沒有
    // 宣告時才依 base_url 推定。宣告比猜準（第三方也可能提供 responses 端點）。
    tbl["wire_api"] = toml_edit::value(auth.wire_api.unwrap_or_else(|| codex_wire_api(base_url)));
    if !inline_models.is_empty() {
        let mut arr = toml_edit::Array::new();
        for (m, display) in inline_models {
            let mut item = toml_edit::InlineTable::new();
            item.insert("model", toml_edit::Value::from(m.clone()));
            item.insert("display_name", toml_edit::Value::from(display.clone()));
            arr.push(toml_edit::Value::InlineTable(item));
        }
        tbl["models"] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
    }
    tbl
}


/// Codex config.toml：toml_edit 手術式改寫，保留註釋與無關段。
/// reasoning 僅在顯式設定（非 unset）時寫入；catalog 為模型目錄路徑時寫入。
/// 共享段名（cc-switch 同款）：`model_provider = "custom"`，custom 與
/// tokengateway 兩段同時指向網關，外加歷史會話用過的 provider 別名段——
/// cc-switch 時代與網關時代的舊會話（按段名引用供應商）都能繼續，
/// 段被外部工具刪除也會在下次接管時重建。
/// 冪等：先刪後插＋回驗，多次接管不疊段；入口先做重複段消毒（非法 TOML 也能救）。
/// [`codex_apply`] 的認證與協議選項。
///
/// 把這兩個欄位綁在一起，是為了讓參數列維持在 8 個（clippy
/// `too_many_arguments` 門檻）—— 原本只有 `direct_key`，加入協議宣告後若
/// 直接再多一個參數就會超標。
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAuth<'a> {
    /// 直連上游 Key（Some = 直連模式：各段寫 `experimental_bearer_token`
    /// 明文，不寫 `env_key`）。
    pub direct_key: Option<&'a str>,
    /// 協議形狀覆寫，來自 provider **宣告**的 `api_format`（§5.3 能力宣告）。
    /// `None` = 依 `base_url` 推定（見 [`codex_wire_api`]）。
    pub wire_api: Option<&'a str>,
}


/// 依 provider 宣告的 `api_format` 決定 Codex 該用哪種 `wire_api`。
///
/// 這是「能力宣告優先於事後猜測」的具體落點：`providers.api_format` 是使用者
/// 對該渠道的**明確宣告**，比從 URL 猜準確（第三方也可能提供 responses
/// 端點）。
///
/// 回傳 `None` = 宣告不足以判定（未宣告，或 Anthropic／Gemini 這種 Codex
/// 根本說不了的協議），交由 [`codex_wire_api`] 依 URL 推定接手。
pub fn codex_wire_api_declared(provider_format: Option<&str>) -> Option<&'static str> {
    match provider_format?.trim() {
        // 只實作 Chat Completions
        "openai-chat" => Some("chat"),
        // 兩種都支援 → 用 Codex 原生的 responses（保留 reasoning 等欄位）
        "mixed" | "openai-responses" => Some("responses"),
        _ => None,
    }
}


/// 產生接管後的 config.toml。
///
/// 參數多達 8 個是**刻意的**：它們各自對應一個獨立的設定面向（既有內容、
/// 上游 URL、模型、推理等級、目錄、別名段、內嵌模型、鑑權），且呼叫端
/// （`switch.rs` 與約 20 條測試）都是逐一具名傳入，比包成 struct 更清楚。
/// 收斂成 struct 只會把同樣的欄位搬個地方，卻讓每個呼叫點多一層縮排。
#[allow(clippy::too_many_arguments)]
pub fn codex_apply(
    existing: &str,
    base_url: &str,
    model: &str,
    reasoning: Option<&str>,
    catalog: Option<&str>,
    aliases: &[String],
    inline_models: &[(String, String)],
    auth: CodexAuth<'_>,
) -> Result<String, String> {
    let clean = dedupe_codex_sections(existing);
    let mut doc: toml_edit::DocumentMut = clean
        .parse()
        .map_err(|e| format!("config.toml 解析失敗：{e}"))?;
    doc["model"] = toml_edit::value(model);
    doc["model_provider"] = toml_edit::value(CODEX_SHARED_PROVIDER_ID);
    if let Some(r) = reasoning {
        if !r.trim().is_empty() && r != "unset" {
            doc["model_reasoning_effort"] = toml_edit::value(r);
        }
    }
    if let Some(c) = catalog {
        if !c.trim().is_empty() {
            doc["model_catalog_json"] = toml_edit::value(c);
        }
    }
    // 顯式建表，保證輸出為 `[model_providers.xxx]` 段頭形式
    //（鏈式索引缺失鍵時會退化成內聯表寫法；注意 Index 讀缺失鍵會 panic，一律用 get）。
    if doc.get("model_providers").is_none() {
        doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let mp = doc["model_providers"]
        .as_table_mut()
        .ok_or("model_providers 不是表，無法接管")?;
    // 先刪後插：冪等，多次接管不疊段
    let mut managed: Vec<String> = vec![
        CODEX_SHARED_PROVIDER_ID.to_string(),
        GATEWAY_PROVIDER_ID.to_string(),
    ];
    for a in aliases {
        let id = a.trim();
        if id.is_empty() || managed.iter().any(|m| m == id) {
            continue;
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        managed.push(id.to_string());
        if managed.len() >= CODEX_ALIAS_CAP + 2 {
            break;
        }
    }
    for id in &managed {
        mp.remove(id.as_str());
    }
    for id in &managed {
        mp.insert(
            id.as_str(),
            toml_edit::Item::Table(gateway_section(base_url, inline_models, auth)),
        );
    }
    let out = doc.to_string();
    // 回驗：託管段頭恰好出現一次，且整檔可再解析；異常則拒絕寫入
    for id in &managed {
        let header = format!("[model_providers.{id}]");
        let n = out.lines().filter(|l| l.trim() == header).count();
        if n != 1 {
            return Err(format!("接管後 {header} 出現 {n} 次，已拒絕寫入"));
        }
    }
    out.parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("接管結果校驗失敗：{e}"))?;
    Ok(out)
}
