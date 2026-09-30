//! TOML 重複段消毒、歷史 provider 別名與內嵌模型讀取。

use super::super::*;

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
            if (name == "model_providers" || name.starts_with("model_providers."))
                && !seen.insert(name)
            {
                skipping = true;
                continue;
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
/// Codex **內建**的 provider id：設定檔只要定義了這些段，**整份** `config.toml`
/// 都會載入失敗，工具表現成「無法登入」。
///
/// 2026-09-29 逐個候選實測（在 `model_provider` 仍指向網關的情況下額外定義一段，
/// 看 Codex 是否拒絕載入）：`openai`／`ollama`／`lmstudio` 會觸發
/// `model_providers contains reserved built-in provider IDs: … Built-in providers
/// cannot be overridden.`；`oss`、`azure`、`aws`、`amazon-bedrock`、`anthropic`、
/// `openrouter`、`google`、`gemini`、`openai-chat`、`openai-custom` 都可以自由定義。
/// 檢查是針對**所有**已定義的段，不限當前 `model_provider`。
///
/// **為什麼會有這份清單（真實事故）**：App 會把「Codex 對話紀錄裡出現過的
/// provider 名」聯集進要保留的別名段（見 `codex_legacy_providers_report` ＋
/// `codex_alias_ids`）。Codex 一旦用**原生**來源跑過，`model_provider = "openai"`
/// 就會出現在對話紀錄裡；下一次接管時 App 會替它**建出** `[model_providers.openai]`
/// ——於是整份設定載入失敗。使用者看到的是「關掉網關回到官方之後，再切回指定來源
/// 就無法登入」。
pub(crate) const CODEX_RESERVED_PROVIDER_IDS: [&str; 3] = ["openai", "ollama", "lmstudio"];


/// 這個 provider id 是不是 Codex 的內建 id（不可在設定檔裡定義）。
pub(crate) fn is_reserved_codex_provider(id: &str) -> bool {
    let id = id.trim();
    CODEX_RESERVED_PROVIDER_IDS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(id))
}


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
///
/// **排除 Codex 內建 id**（`CODEX_RESERVED_PROVIDER_IDS`）：那些名字不可在設定檔
/// 裡定義，聯集進來會讓整份設定載入失敗（真實事故，見該常數的說明）。
pub(crate) fn codex_alias_ids(existing: &str, from_db: &[String], gw_url: &str) -> Vec<String> {
    let mut out: Vec<String> = from_db
        .iter()
        .filter(|o| !is_reserved_codex_provider(o))
        .cloned()
        .collect();
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
            || is_reserved_codex_provider(id)
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
