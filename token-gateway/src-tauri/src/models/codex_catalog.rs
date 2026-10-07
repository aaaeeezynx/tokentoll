//! Codex 模型目錄生成（原 `models.rs` 的「Codex 目錄生成」段）。

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use super::crud::models_list;

// ------------------------------------------------------- Codex 目錄生成 ---

/// 用戶現有 catalog 模板路徑（cc-switch 生成、與其 Codex 同版本；僅遷移期回退）。
fn legacy_template_path(home: &Path) -> PathBuf {
    home.join(".codex").join("cc-switch-model-catalog.json")
}

/// 網關自持模板（收編 cc-switch 模板後不再依賴 cc-switch 存在）。
fn gateway_template_path(app_data: &Path) -> PathBuf {
    app_data.join("catalogs").join("codex-template.json")
}

/// 找一條跑得起來的 Codex CLI 命令列。
///
/// 三種來源，依序試：
/// 1. PATH 上的 `codex.exe`
/// 2. **Codex 桌面版**自帶的 `%LOCALAPPDATA%\OpenAI\Codex\bin\<版本>\codex.exe`
///    —— 它不在 PATH 上，但實測 `debug models --bundled` 可用，而很多使用者
///    只裝桌面版、沒裝 npm 的 CLI
/// 3. PATH 上的 npm shim `codex.cmd` —— batch 不能直接 `CreateProcess`，要經 `cmd /c`
///
/// `.ps1` shim 刻意不採用：`std::process::Command` 跑不起來，繞 PowerShell 又多一層殼。
fn codex_argv() -> Option<Vec<String>> {
    let tail = ["debug", "models", "--bundled"];

    #[cfg(windows)]
    {
        if let Some(p) = crate::tools::which_bin(&["codex.exe"]) {
            let mut v = vec![p.to_string_lossy().to_string()];
            v.extend(tail.map(str::to_string));
            return Some(v);
        }
        if let Some(p) = desktop_codex_exe() {
            let mut v = vec![p.to_string_lossy().to_string()];
            v.extend(tail.map(str::to_string));
            return Some(v);
        }
        if let Some(p) = crate::tools::which_bin(&["codex.cmd"]) {
            let mut v = vec!["cmd".to_string(), "/c".to_string(), p.to_string_lossy().to_string()];
            v.extend(tail.map(str::to_string));
            return Some(v);
        }
        None
    }
    #[cfg(not(windows))]
    {
        let p = crate::tools::which_bin(&["codex"])?;
        let mut v = vec![p.to_string_lossy().to_string()];
        v.extend(tail.map(str::to_string));
        Some(v)
    }
}

/// Codex 桌面版的 `codex.exe`（版本化目錄，取最新的那個）。
#[cfg(windows)]
fn desktop_codex_exe() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?.join("OpenAI").join("Codex").join("bin");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(&base).ok()?.flatten() {
        let exe = e.path().join("codex.exe");
        let Ok(md) = std::fs::metadata(&exe) else {
            continue;
        };
        let t = md.modified().unwrap_or(std::time::UNIX_EPOCH);
        let better = match &best {
            None => true,
            Some((bt, _)) => t > *bt,
        };
        if better {
            best = Some((t, exe));
        }
    }
    best.map(|(_, p)| p)
}

/// 從 Codex CLI 匯出內建模型目錄當模板。
///
/// `codex debug models --bundled` 會把 bundled catalog 以 JSON 印到 stdout，
/// 頂層就是 `{"models":[…]}` —— 與模板格式完全相同，所以可以直接當模板用。
///
/// 這條路徑讓「產生模型目錄」**不再依賴 cc-switch**。
pub(crate) fn export_template_from_codex() -> Result<Vec<u8>, String> {
    let argv = codex_argv().ok_or_else(|| {
        "PATH 上找不到 codex.exe／codex.cmd，也沒有 Codex 桌面版的 codex.exe".to_string()
    })?;
    // 用專案既有的帶逾時執行器：它已處理 Windows 的 CREATE_NO_WINDOW 與逾時擊殺，
    // 免得 codex 卡住時把整個接管流程一起拖住。
    let text = crate::tools::run_with_timeout(&argv, 30_000)?;
    // 去 BOM：不同殼層版本可能加上。網關稍後用 read_to_string + serde_json 讀它，
    // 而 serde_json **不接受** BOM（見 `proxy/strip.rs` 為何要手動 strip）。
    let text = text.trim_start_matches('\u{feff}');
    validate_template(text)?;
    Ok(text.as_bytes().to_vec())
}

/// 確認拿到的內容真的是可用的模板（頂層 `models` 非空陣列）。
///
/// 沒有這道檢查，一個「找不到模型」的錯誤訊息會被當成模板收下，
/// 之後才在目錄生成時以更難懂的方式爆掉。
pub(crate) fn validate_template(text: &str) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| format!("不是合法 JSON：{e}"))?;
    match v.get("models").and_then(|m| m.as_array()) {
        Some(a) if !a.is_empty() => Ok(()),
        _ => Err("沒有 `models` 陣列（或它是空的）".to_string()),
    }
}

/// 取得可用的模板路徑，必要時自己生一份。
///
/// 依序：① 網關自持副本 ② 收編 cc-switch 遺留檔 ③ **自己跟 Codex 要**。
///
/// 第 ③ 條是 2026-10-08 補的。在那之前只做到 ②，於是「從沒裝過 cc-switch」的
/// 乾淨機器（= 第一次安裝的使用者）必定失敗；而錯誤訊息叫他跑的
/// `codex debug models --bundled` **只印到 stdout、不寫檔**，照著做也生不出
/// 網關要找的那個檔案 —— 指示本身是錯的。
pub(crate) fn ensure_template(home: &Path, app_data: &Path) -> Result<PathBuf, String> {
    ensure_template_with(home, app_data, &export_template_from_codex)
}

/// [`ensure_template`] 的注入版（測試用 stub 取代真的呼叫 Codex）。
pub(crate) fn ensure_template_with(
    home: &Path,
    app_data: &Path,
    export: &dyn Fn() -> Result<Vec<u8>, String>,
) -> Result<PathBuf, String> {
    let gw = gateway_template_path(app_data);
    if gw.exists() {
        return Ok(gw);
    }
    let legacy = legacy_template_path(home);
    if legacy.exists() {
        if let Some(parent) = gw.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&legacy, &gw).map_err(|e| format!("模板收編失敗：{e}"))?;
        return Ok(gw);
    }
    let bytes = export().map_err(|e| {
        format!(
            "找不到模型目錄模板，且無法從 Codex 自動取得：{e}\n\
             · 解法一：安裝 Codex CLI（`npm i -g @openai/codex`）或 Codex 桌面版後重試；\n\
             · 解法二：在接管對話框取消勾選「同時產生 Codex 模型目錄」——\
             接管仍會完成，Codex 改用自己內建的模型清單。"
        )
    })?;
    validate_template(&String::from_utf8_lossy(&bytes))?;
    if let Some(parent) = gw.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::fsutil::atomic_write(&gw, &bytes).map_err(|e| format!("寫入模板失敗：{e}"))?;
    Ok(gw)
}

fn level_fallback_desc(e: &str) -> &'static str {
    match e {
        "minimal" => "Very light reasoning for speed",
        "low" => "Light reasoning for everyday tasks",
        "medium" => "Balances speed and reasoning depth for everyday tasks",
        "high" => "Greater reasoning depth for complex problems",
        "xhigh" => "Extra high reasoning depth for complex problems",
        _ => "Maximum reasoning depth",
    }
}

/// 模板各模型思考等級並集（含描述），規範序在前、模板自帶等級按出現序附加。
/// 描述缺失時補標準文案（Codex 目錄要求每檔帶描述）。
fn template_levels_union(tpl: &serde_json::Value) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = vec![];
    if let Some(models) = tpl.get("models").and_then(|m| m.as_array()) {
        for m in models {
            if let Some(levels) = m.get("supported_reasoning_levels").and_then(|l| l.as_array()) {
                for lv in levels {
                    let e = lv.get("effort").and_then(|x| x.as_str()).unwrap_or("");
                    if e.is_empty() {
                        continue;
                    }
                    let d = lv.get("description").and_then(|x| x.as_str()).unwrap_or("");
                    match found.iter_mut().find(|(fe, _)| fe == e) {
                        Some(slot) => {
                            if slot.1.is_empty() && !d.is_empty() {
                                slot.1 = d.to_string();
                            }
                        }
                        None => found.push((e.to_string(), d.to_string())),
                    }
                }
            }
        }
    }
    const ORDER: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];
    let mut out: Vec<(String, String)> = ORDER
        .iter()
        .filter_map(|e| {
            found
                .iter()
                .find(|(fe, _)| fe == e)
                .map(|(fe, fd)| (fe.clone(), fd.clone()))
        })
        .collect();
    for (e, d) in found {
        if !out.iter().any(|(fe, _)| *fe == e) {
            out.push((e, d));
        }
    }
    if !out.is_empty() {
        for (e, d) in out.iter_mut() {
            if d.is_empty() {
                *d = level_fallback_desc(e).to_string();
            }
        }
    }
    out
}

/// 全部預設檔位：模板並集；模板無檔位信息時退回標準五檔。
fn all_levels(union: &[(String, String)]) -> Vec<(String, String)> {
    if union.is_empty() {
        ["minimal", "low", "medium", "high", "xhigh"]
            .iter()
            .map(|e| (e.to_string(), level_fallback_desc(e).to_string()))
            .collect()
    } else {
        union.to_vec()
    }
}

/// 依模型 reasoning 值算目錄檔位：unset/空 = 全部預設檔位；
/// 指定 = 按規範序過濾到已知檔位（拼錯值丟棄），全被丟棄時退回全部預設。
/// 返回 (檔位列表, 是否顯式指定)。
fn levels_for(
    reasoning: &str,
    union: &[(String, String)],
) -> (Vec<(String, String)>, bool) {
    let known = all_levels(union);
    let spec: Vec<String> = reasoning
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && s != "unset")
        .collect();
    if spec.is_empty() {
        return (known, false);
    }
    let picked: Vec<(String, String)> = known
        .iter()
        .filter(|(fe, _)| spec.contains(fe))
        .cloned()
        .collect();
    if picked.is_empty() {
        return (known, false);
    }
    (picked, true)
}

/// 模型沒填 `context_window` 時，目錄寫入的預設值（256K）。
///
/// **刻意不沿用模板值**：模板（`codex-template.json`）是收編 cc-switch 的產物，
/// 裡面寫死 `context_window: 1000000`。使用者沒填的模型若原樣繼承 1M，Codex 會
/// 以為有 100 萬 token 可用而遲不壓縮，把請求堆到上游直接 400 —— 2026-10-06 實測
/// 出現過 937 KB／2.3 MB／甚至 25 MB 的請求體（`proxy_trace.content_length`）。
///
/// 256K 是保守且常見的起點；要更大請在「來源 → 使用模型」逐模型填實際值。
pub(crate) const DEFAULT_CONTEXT_WINDOW: i64 = 262_144;

/// 模板複製生成 Codex 模型目錄：改 slug/顯示名/描述/上下文/思考等級，
/// 其餘上百欄位原樣保留（版本安全）。
/// - 模型聯合目錄：當前來源啟用模型在前，其他啟用來源的模型一併收錄
///   （Codex 端任意來源的模型 slug 都能解析；跨來源請求由網關按模型路由）。
/// - 思考等級：模型未指定（unset/空）= 全部預設檔位；指定 = 按規範序過濾。
/// - 上下文視窗：模型有填就用它，沒填用 [`DEFAULT_CONTEXT_WINDOW`]（**不繼承模板**）。
pub(crate) fn codex_catalog_generate(
    conn: &Connection,
    home: &Path,
    app_data: &Path,
    provider_id: i64,
    // true = 聯合其他啟用來源的模型（Codex 內一次看到全庫）；
    // false = 僅當前來源（避免選擇器被其他來源污染）。
    union_others: bool,
) -> Result<PathBuf, String> {
    let text = std::fs::read_to_string(ensure_template(home, app_data)?).map_err(|e| {
        format!("模板讀取失敗：{e}")
    })?;
    let tpl: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("模板解析失敗：{e}"))?;
    let first = tpl
        .get("models")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .ok_or("模板無 models 條目".to_string())?
        .clone();
    let union = template_levels_union(&tpl);
    // 當前來源啟用模型（目錄主體，排在前）
    let mut rows: Vec<(String, String, Option<i64>, String)> = models_list(conn, provider_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|r| r.enabled)
        .map(|r| (r.display_name, r.actual_model, r.context_window, r.reasoning))
        .collect();
    // 其他啟用來源的模型（聯合目錄；關閉時僅當前來源，避免污染選擇器）
    let others: Vec<(String, String, Option<i64>, String)> = if union_others {
        let mut stmt = conn
            .prepare(
                "SELECT m.display_name, m.actual_model, m.context_window, m.reasoning
                 FROM provider_models m JOIN providers p ON p.id = m.provider_id
                 WHERE m.enabled = 1 AND p.enabled = 1 AND m.provider_id != ?1
                 ORDER BY p.priority ASC, p.id ASC, m.ord ASC, m.id ASC",
            )
            .map_err(|e| e.to_string())?;
        // 先綁局部再返回：尾表達式的 `?` 臨時值否則活得比 stmt 久（E0597）
        let mapped: Vec<(String, String, Option<i64>, String)> = stmt
            .query_map(params![provider_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|x| x.ok())
            .collect();
        mapped
    } else {
        vec![]
    };
    rows.extend(others);
    if rows.is_empty() {
        return Err("沒有任何啟用來源登記了可用模型".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for (display, actual, cw, reasoning) in rows {
        let slug = actual.trim().to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
            continue;
        }
        let mut e = first.clone();
        e["slug"] = serde_json::Value::String(slug.clone());
        // issue #3668：桌面版 /model 與選擇器認 `model` 欄位，只寫 slug 會顯示空白
        e["model"] = serde_json::Value::String(slug.clone());
        e["display_name"] = serde_json::Value::String(display);
        e["description"] = serde_json::Value::String(slug);
        // 上下文視窗：有填用填的，沒填用 256K 預設。
        // 舊版是「None 就不覆寫」→ 模板的 1M 會漏進每一個沒填的模型（見常數註解）。
        let cwv = cw.filter(|n| *n > 0).unwrap_or(DEFAULT_CONTEXT_WINDOW);
        e["context_window"] = serde_json::Value::from(cwv);
        e["max_context_window"] = serde_json::Value::from(cwv);
        let (levels, explicit) = levels_for(&reasoning, &union);
        e["supported_reasoning_levels"] = serde_json::Value::Array(
            levels
                .iter()
                .map(|(ef, d)| serde_json::json!({"effort": ef, "description": d}))
                .collect(),
        );
        if explicit {
            if let Some((first_lv, _)) = levels.first() {
                e["default_reasoning_level"] = serde_json::Value::String(first_lv.clone());
            }
        }
        out.push(e);
    }
    let dir = app_data.join("catalogs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("codex-{provider_id}.json"));
    let body = serde_json::to_string_pretty(&serde_json::json!({"models": out}))
        .map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write(&dest, body.as_bytes()).map_err(|e| e.to_string())?;
    Ok(dest)
}
