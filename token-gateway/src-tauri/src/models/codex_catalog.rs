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

/// 模板收編：網關自持副本缺失時，從 cc-switch 遺留檔案複製一份（一次性遷移）。
pub(crate) fn ensure_template(home: &Path, app_data: &Path) -> Result<PathBuf, String> {
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
    Err("找不到模型目錄模板：網關 catalogs/codex-template.json 與 ~/.codex/cc-switch-model-catalog.json 均缺失，請先跑一次 `codex debug models --bundled` 導出".to_string())
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

/// 模板複製生成 Codex 模型目錄：改 slug/顯示名/描述/上下文/思考等級，
/// 其餘上百欄位原樣保留（版本安全）。
/// - 模型聯合目錄：當前來源啟用模型在前，其他啟用來源的模型一併收錄
///   （Codex 端任意來源的模型 slug 都能解析；跨來源請求由網關按模型路由）。
/// - 思考等級：模型未指定（unset/空）= 全部預設檔位；指定 = 按規範序過濾。
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
        if let Some(cwv) = cw {
            e["context_window"] = serde_json::Value::from(cwv);
            e["max_context_window"] = serde_json::Value::from(cwv);
        }
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
