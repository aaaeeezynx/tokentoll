//! Codex 接管（config.toml、別名、協議、歷史 provider）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

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

// ---------------------------------------------------------------- 檢測 ---


/// 文本級去重：丟棄第 2 個及之後重複的 `[model_providers.X]` 整段
///（多次接管 / 外部工具疊寫產生的非法 TOML，先消毒再解析；首段勝出）。
pub(crate) fn dedupe_codex_sections(text: &str) -> String {
    let mut out = String::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && !t.starts_with("[[") && t.ends_with(']') {
            let name = t[1..t.len() - 1].trim().to_string();
            if name == "model_providers" || name.starts_with("model_providers.") {
                if !seen.insert(name) {
                    skipping = true;
                    continue;
                }
            }
            skipping = false;
        }
        if skipping {
            continue;
        }
        out.push_str(line);
    }
    out
}


/// 讀 Codex 歷史會話 provider 名的結果。
///
/// **為什麼需要這個型別**：原本的實作在讀不到時回傳空 `Vec`，與「真的沒有
/// 第三方 provider 殘留」完全無法區分。`codex_doctor()` 因此會在**讀取失敗
/// 時報「✅ 歷史會話無第三方 provider 殘留」** —— 把一個失敗報成通過。當
/// 使用者說「舊會話無法續用」時，唯一的診斷工具會告訴他一切正常，這是這個
/// 問題最難查的原因。
#[derive(Debug)]
pub enum LegacyProviders {
    /// 讀成功。`providers` 可能為空（＝真的沒有殘留）。
    Ok { providers: Vec<String>, db: PathBuf },
    /// 讀失敗，附可讀原因。**呼叫端不可把它靜默當成空。**
    Failed { reason: String },
}


/// 找出 Codex 狀態資料庫的候選，依版號由高到低。
///
/// **不可寫死 `state_5.sqlite`。** Codex 家目錄裡的檔名全部帶版本後綴
/// （`state_5`、`logs_2`、`goals_1`、`queue_1`、`memories_1`、
/// `thread_history_1`），那是它的 schema 版號。一旦 Codex 升版把 `state_5`
/// 換成 `state_6`，寫死路徑的程式就會**靜默回傳空 vec**，別名段全部消失，
/// 舊會話即無法續用 —— 而且是無聲無息地發生。這裡改成掃描所有
/// `state*.sqlite`。
pub(crate) fn codex_state_db_candidates(codex_home: &Path) -> Vec<PathBuf> {
    let mut cands: Vec<(u32, PathBuf)> = vec![];
    let Ok(rd) = std::fs::read_dir(codex_home) else {
        return vec![];
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("state") else {
            continue;
        };
        let Some(num) = rest.strip_suffix(".sqlite") else {
            continue;
        };
        // `state.sqlite` 視為 0、`state_5.sqlite` 為 5；其餘形式不認。
        let n: u32 = if num.is_empty() {
            0
        } else if let Some(d) = num.strip_prefix('_') {
            match d.parse() {
                Ok(v) => v,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        cands.push((n, e.path()));
    }
    cands.sort_by_key(|c| std::cmp::Reverse(c.0));
    cands.into_iter().map(|(_, p)| p).collect()
}


/// 讀單一 DB 的 provider 名；失敗時回傳**原因**（不再吞掉）。
pub(crate) fn read_legacy_providers_from(db: &Path) -> Result<Vec<String>, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("唯讀開啟失敗：{e}"))?;
    // Codex 隨時在寫這個檔（實測 `-wal` 可達 2 MB、`-shm` 一直在動）。
    // 沒有 busy_timeout 的話一遇鎖就直接失敗 —— 而失敗以前是靜默的。
    conn.busy_timeout(std::time::Duration::from_millis(3000))
        .map_err(|e| format!("設定 busy_timeout 失敗：{e}"))?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT model_provider FROM threads")
        .map_err(|e| format!("沒有 threads 表或查詢無法準備：{e}"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("查詢失敗：{e}"))?;
    let mut out: Vec<String> = vec![];
    let mut row_errs = 0usize;
    for r in rows {
        // 以前是 `rows.flatten()`，這一類錯誤被整個吞掉（第 5 條靜默路徑）。
        let id = match r {
            Ok(v) => v.trim().to_string(),
            Err(_) => {
                row_errs += 1;
                continue;
            }
        };
        if id.is_empty() || id == CODEX_SHARED_PROVIDER_ID || id == GATEWAY_PROVIDER_ID {
            continue;
        }
        // 僅保留合法 TOML 鍵字元
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        if !out.contains(&id) {
            out.push(id);
        }
        if out.len() >= CODEX_ALIAS_CAP {
            break;
        }
    }
    // 有列但全讀不出來 → 這是失敗，不是「沒有殘留」。
    if out.is_empty() && row_errs > 0 {
        return Err(format!("{row_errs} 列的 model_provider 都讀不出來"));
    }
    Ok(out)
}


/// 讀 Codex 歷史會話用過的 provider 名，並**明確區分成功與失敗**。
///
/// 只讀不寫。逐個候選 DB（版號高者優先）嘗試，第一個成功者勝出；全部失敗
/// 時回傳 [`LegacyProviders::Failed`] 並附完整原因。
pub fn codex_legacy_providers_report(codex_home: &Path) -> LegacyProviders {
    let cands = codex_state_db_candidates(codex_home);
    if cands.is_empty() {
        return LegacyProviders::Failed {
            reason: format!(
                "在 {} 找不到任何 state*.sqlite（Codex 尚未產生，或改了檔名規則）",
                codex_home.display()
            ),
        };
    }
    let mut why: Vec<String> = vec![];
    for db in &cands {
        match read_legacy_providers_from(db) {
            Ok(providers) => {
                return LegacyProviders::Ok {
                    providers,
                    db: db.clone(),
                }
            }
            Err(e) => why.push(format!(
                "{}：{e}",
                db.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    LegacyProviders::Failed {
        reason: format!(
            "找到 {} 個 state*.sqlite 但都讀不出 provider：{}",
            cands.len(),
            why.join("；")
        ),
    }
}


/// 要管理的別名段名 = （DB 讀到的 provider 名）∪（config.toml 裡**已經指向
/// 本網關**的 `[model_providers.*]` 段名）。
///
/// **為什麼要聯集**：`codex_apply` 只會刪除「在管理清單裡」的段。若清單只來自
/// DB，一旦 threads 變少（例如使用者**封存對話**之後，那個 provider 不再出現
/// 在 `SELECT DISTINCT model_provider` 裡），本來存在的別名段就會被剔除，
/// 舊會話的 provider 段因而消失。聯集讓已由本工具接管的段**只增不減**，這正
/// 是「舊會話必須一直能續用」所需要的性質。
///
/// **只納入 base_url 已經指向本網關的段**，不碰使用者自己指向上游的 provider
/// —— 否則接管會把「使用者想直連」的段也一併劫持。
pub(crate) fn codex_alias_ids(existing: &str, from_db: &[String], gw_url: &str) -> Vec<String> {
    let mut out: Vec<String> = from_db.to_vec();
    let clean = dedupe_codex_sections(existing);
    let Ok(doc) = clean.parse::<toml_edit::DocumentMut>() else {
        return out;
    };
    let Some(mp) = doc.get("model_providers").and_then(|m| m.as_table()) else {
        return out;
    };
    for (k, v) in mp.iter() {
        let id = k.trim();
        if id.is_empty()
            || id == CODEX_SHARED_PROVIDER_ID
            || id == GATEWAY_PROVIDER_ID
            || out.iter().any(|o| o == id)
        {
            continue;
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        // 只接管本來就指向本網關的段。
        if v.get("base_url").and_then(|b| b.as_str()) != Some(gw_url) {
            continue;
        }
        out.push(id.to_string());
    }
    out
}


/// 從已生成的 catalog 讀 (slug, display_name)，寫入 provider 段內聯 models（桌面版選擇器顯示用）。
/// catalog 缺失 / 解析失敗時返回空（不阻塞接管）。
pub fn codex_inline_models(catalog: Option<&str>) -> Vec<(String, String)> {
    let path = match catalog {
        Some(c) if !c.trim().is_empty() => c,
        _ => return vec![],
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v.get("models").and_then(|m| m.as_array()) {
        Some(a) => a,
        None => return vec![],
    };
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for e in arr {
        let slug = e
            .get("slug")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
            continue;
        }
        let display = e
            .get("display_name")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .trim();
        let name = if display.is_empty() {
            slug.clone()
        } else {
            display.to_string()
        };
        out.push((slug, name));
        if out.len() >= CODEX_INLINE_MODELS_CAP {
            break;
        }
    }
    out
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


/// Codex 接管前體檢：返回逐行結論（✅ 正常 / ⚠️ 接管時處理 / ❌ 先手工排除）。
/// 純讀取，不寫任何檔案。
pub fn codex_doctor_at(codex_home: &Path, port: u16) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let gw = format!("http://{GATEWAY_HOST}:{port}/v1");
    let cfg_path = codex_home.join("config.toml");
    match std::fs::read_to_string(&cfg_path) {
        Err(_) => out.push("⚠️ config.toml 不存在：接管將新建".to_string()),
        Ok(t) => {
            for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                let header = format!("[model_providers.{id}]");
                let n = t.lines().filter(|l| l.trim() == header).count();
                if n > 1 {
                    out.push(format!(
                        "❌ {header} 重複 {n} 次（非法 TOML）：接管入口將自動去重消毒"
                    ));
                }
            }
            match dedupe_codex_sections(&t).parse::<toml_edit::DocumentMut>() {
                Err(e) => out.push(format!("❌ config.toml 解析失敗：{e}")),
                Ok(doc) => {
                    if doc.get("model_provider").and_then(|v| v.as_str())
                        != Some(CODEX_SHARED_PROVIDER_ID)
                    {
                        out.push("⚠️ model_provider 不是 custom：接管將切到共享段名".to_string());
                    }
                    for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                        match doc
                            .get("model_providers")
                            .and_then(|m| m.get(id))
                            .and_then(|s| s.get("base_url"))
                            .and_then(|u| u.as_str())
                        {
                            Some(u) if u == gw => {}
                            Some(u) => out.push(format!(
                                "⚠️ [model_providers.{id}] base_url = {u}，不是本網關 {gw}（接管時將覆寫為網關；僅提示，不影響開關按鈕）"
                            )),
                            None => {
                                out.push(format!("⚠️ [model_providers.{id}] 缺失：接管將補建"))
                            }
                        }
                    }
                    match doc.get("model_catalog_json").and_then(|v| v.as_str()) {
                        Some(c) if !std::path::Path::new(c).exists() => {
                            out.push(format!("⚠️ model_catalog_json 指向缺失：{c}（接管將重寫）"))
                        }
                        None => out.push("⚠️ 未設 model_catalog_json：接管將按來源生成".to_string()),
                        _ => {}
                    }
                }
            }
        }
    }
    // **這裡以前會把讀取失敗報成 ✅。** 讀不到時 `codex_legacy_providers()`
    // 回傳空 vec，於是體檢顯示「歷史會話無第三方 provider 殘留」——一個失敗
    // 被當成通過，使用者唯一的診斷工具反而誤導他。現在失敗一律 ❌ 並附原因。
    match codex_legacy_providers_report(codex_home) {
        LegacyProviders::Failed { reason } => out.push(format!(
            "❌ 讀不到 Codex 歷史會話的 provider 名：{reason}。接管將無法補寫別名段，\
             仍在使用舊會話的 provider 可能失效 —— 請完全結束 Codex 後重試"
        )),
        LegacyProviders::Ok { providers, db } => {
            let src = db.file_name().unwrap_or_default().to_string_lossy().to_string();
            if providers.is_empty() {
                out.push(format!(
                    "✅ 歷史會話無第三方 provider 殘留（custom / tokengateway 之外；讀自 {src}）"
                ));
            } else {
                out.push(format!(
                    "ℹ️ 歷史會話用過 {} 個別名 provider（讀自 {src}）：{} —— 接管將全寫為網關別名段",
                    providers.len(),
                    providers.join(", ")
                ));
            }
        }
    }
    if std::env::var(GATEWAY_ENV_KEY)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        out.push(format!("✅ {GATEWAY_ENV_KEY} 已在環境變數"));
    } else {
        out.push(format!(
            "❌ {GATEWAY_ENV_KEY} 未設定：Codex 行程繼承不到 Key 會 401，請先設為用戶環境變數"
        ));
    }
    // cc-switch 代理殘留（15721）會與接管預期衝突
    let probe: std::net::SocketAddr = format!("{GATEWAY_HOST}:15721").parse().unwrap();
    if std::net::TcpStream::connect_timeout(&probe, std::time::Duration::from_millis(300)).is_ok()
    {
        out.push("❌ 15721 被佔用（疑似 cc-switch 代理仍在跑）：先完全退出 cc-switch 再接管".to_string());
    } else {
        out.push("✅ 15721 空閒（cc-switch 代理未運行）".to_string());
    }
    out
}


/// Codex 接管前體檢（用戶主目錄版）。
pub fn codex_doctor(port: u16) -> Vec<String> {
    match user_home() {
        Ok(h) => codex_doctor_at(&h.join(".codex"), port),
        Err(e) => vec![format!("❌ 用戶目錄不可用：{e}")],
    }
}
