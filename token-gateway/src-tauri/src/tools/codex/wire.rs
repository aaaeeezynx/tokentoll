//! 網關 URL、協議推定與 Codex `config.toml` 改寫。

use super::super::*;
use super::legacy::dedupe_codex_sections;
use super::CODEX_RESERVED_PROVIDER_IDS;

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


/// **寫進 Codex `config.toml` 的 `wire_api` 一律是這個值。**
///
/// 為什麼是一個常數，而不是「依上游能力判斷」的函式（2026-09-28 定案）：
///
/// 1. 本 App **唯一的模式就是經網關** —— `plan_switch` 與 `apply_switch`
///    都會把 `base_url` 強制改寫成 `gateway_url(...)`（「真·直連」分支已於
///    2026-09-28 移除，見 `docs/SIMPLIFICATION-PLAN.md` §4.4）。所以寫入時
///    看到的 URL **永遠**是本網關。
/// 2. `wire_api` 描述的是 **Codex → 網關** 這一段，不是 網關 → 上游。
///    網關兩種協議都收並代為轉譯，用 Codex 原生的 `responses` 最好
///    （保留 reasoning 等欄位）。
/// 3. **Codex 已不再接受 `wire_api = "chat"`。** 它讀到就整份 config 判為
///    「Invalid configuration; using defaults」，於是回頭找 ChatGPT 登入，
///    使用者看到的錯誤是 **「Unable to log in」** —— 完全看不出真正起因。
///
/// 第 3 點是 2026-09-28 的真實故障：舊版依 provider 宣告的 `api_format`
/// 選協議，而 7 個來源**全部**宣告 `openai-chat`，於是接管後 8 個 provider
/// 段全被寫成 `chat`，Codex 直接罷工。詳見 `docs/TESTING.md` §0.6。
///
/// **刻意用常數而不是函式**：寫入端（[`gateway_section`]）與畫面預覽
/// （`switch.rs::plan_switch`）都要用它，兩邊各算一次就會漂移 ——
/// 這個 bug 當天就發生過一次（寫入端修好了、預覽端還在顯示 `chat`）。
/// 常數讓這種漂移在型別層面不可能發生。
pub const CODEX_WIRE_API: &str = "responses";


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
    // 協議形狀：**一律** `CODEX_WIRE_API`。指向網關就是唯一模式，
    // 而上游宣告與此無關（Codex 更是已不接受 `chat`）—— 見常數的說明。
    tbl["wire_api"] = toml_edit::value(CODEX_WIRE_API);
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
/// [`codex_apply`] 的認證選項。
///
/// 只有一個欄位，但**刻意不收斂成裸參數**：`codex_apply` 的參數列已經在
/// 8 個（clippy `too_many_arguments` 門檻）而不能再多一個。若以後還要加
/// 認證面向（例如改走 OAuth），直接往這裡加欄位即可，不必再動參數列。
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAuth<'a> {
    /// 直連上游 Key（Some = 直連模式：各段寫 `experimental_bearer_token`
    /// 明文，不寫 `env_key`）。
    pub direct_key: Option<&'a str>,
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
    // 清掉 Codex 內建 id 的段：那些名字不可定義，留著會讓**整份** config.toml
    // 載入失敗（Codex 會報 `reserved built-in provider IDs`，使用者看到的是
    // 「無法登入」）。舊版 App 或歷史殘留可能已經寫進去，所以這裡是修復點。
    for id in CODEX_RESERVED_PROVIDER_IDS {
        mp.remove(id);
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
