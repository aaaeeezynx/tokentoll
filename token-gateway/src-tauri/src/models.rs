//! M6：模型目錄抓取 + 可用模型映射（對標 cc-switch 模型映射）。
//!
//! - 一鍵抓取上游 `/models`（OpenAI 兼容 `{data[]}` / Gemini `{models[]}`），緩存 ID 列表。
//! - 可用模型表：選單顯示名 / 實際請求模型 / 上下文視窗 / 思考等級 / 啟用。
//! - Codex 模型目錄：模板複製用户現有 catalog（版本安全），只改 slug/顯示名/
//!   上下文/思考等級，其餘上百欄位原樣保留。

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 思考等級（七檔；unset = 不指定，目錄使用全部預設檔位）。
pub const REASONINGS: [&str; 7] = [
    "unset", "minimal", "low", "medium", "high", "xhigh", "max",
];

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogCache {
    pub provider_id: i64,
    pub fetched_at: i64,
    pub models: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsableModel {
    pub id: i64,
    pub provider_id: i64,
    pub display_name: String,
    pub actual_model: String,
    pub context_window: Option<i64>,
    pub reasoning: String,
    pub ord: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsableModelInput {
    pub provider_id: i64,
    pub display_name: String,
    pub actual_model: String,
    pub context_window: Option<i64>,
    pub reasoning: String,
    pub ord: i64,
    pub enabled: bool,
}

fn catalog_url(base: &str, api_format: &str) -> String {
    let b = base.trim_end_matches('/');
    if api_format == "gemini" {
        format!("{b}/v1beta/models")
    } else if api_format == "anthropic" {
        format!("{b}/v1/models")
    } else {
        format!("{b}/models")
    }
}

/// 上游錯誤轉中文提示：狀態碼 + 回包前 200 字（真正原因常寫在 body 裡，
/// 如 Google 無 key 時的 unregistered callers）。
fn http_err(resp: reqwest::blocking::Response) -> String {
    let status = resp.status();
    let snippet: String = resp.text().unwrap_or_default().chars().take(200).collect();
    let snippet = snippet.trim();
    if snippet.is_empty() {
        format!("抓取失敗 HTTP {status}")
    } else {
        format!("抓取失敗 HTTP {status}：{snippet}")
    }
}

/// 解析模型 ID 列表（OpenAI `{data[].id}` / Gemini `{models[].name}` 去 `models/` 前綴）。
pub(crate) fn parse_catalog(
    v: &serde_json::Value,
    api_format: &str,
) -> Result<Vec<String>, String> {
    let mut ids = vec![];
    if api_format == "gemini" {
        let arr = v
            .get("models")
            .and_then(|m| m.as_array())
            .ok_or("Gemini 回包無 models 數組".to_string())?;
        for m in arr {
            if let Some(n) = m.get("name").and_then(|n| n.as_str()) {
                let id = n.strip_prefix("models/").unwrap_or(n).trim();
                if !id.is_empty() && !ids.contains(&id.to_string()) {
                    ids.push(id.to_string());
                }
            }
        }
    } else {
        let arr = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or("回包無 data 數組（非 OpenAI 兼容格式？）".to_string())?;
        for m in arr {
            if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                let id = id.trim();
                if !id.is_empty() && !ids.contains(&id.to_string()) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    if ids.is_empty() {
        return Err("未解析到任何模型 ID".to_string());
    }
    Ok(ids)
}

pub(crate) fn catalog_get(
    conn: &Connection,
    provider_id: i64,
) -> rusqlite::Result<CatalogCache> {
    let (fetched_at, raw): (i64, String) = conn.query_row(
        "SELECT fetched_at, models_json FROM model_catalogs WHERE provider_id = ?1",
        [provider_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let ids: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    Ok(CatalogCache {
        provider_id,
        fetched_at,
        models: ids.into_iter().map(|id| CatalogEntry { id }).collect(),
    })
}

/// 渠道目錄抓取配置（讀段產物，供純網路段使用）。
pub(crate) struct CatalogCfg {
    pub base: String,
    pub key: String,
    pub scheme: String,
    pub format: String,
}

/// 短鎖段：讀渠道配置 + 覆寫檢查（不做網路）。
pub(crate) fn catalog_read_cfg(
    conn: &Connection,
    provider_id: i64,
    api_key_override: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<CatalogCfg, String> {
    let (mut base, mut key, scheme, format): (String, String, String, String) = conn
        .query_row(
            "SELECT base_url, api_key, auth_scheme, api_format FROM providers WHERE id = ?1",
            [provider_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| format!("渠道不存在：{provider_id}"))?;
    if let Some(override_key) = api_key_override {
        if !override_key.trim().is_empty() {
            key = override_key.to_string();
        }
    }
    // 來源編輯表允許「改完 URL 不點儲存直接抓取」：base 覆蓋與 key 覆蓋同理，
    // 否則會拿已儲存的舊 URL 配新 Key 去抓，抓回別家的模型目錄。
    if let Some(override_base) = base_url_override {
        if !override_base.trim().is_empty() {
            base = override_base.trim().to_string();
        }
    }
    if key.trim().is_empty() {
        return Err("該渠道尚未填寫 API Key，請先編輯渠道填入後再抓取".to_string());
    }
    Ok(CatalogCfg {
        base,
        key,
        scheme,
        format,
    })
}

/// 純網路段：請求上游模型目錄（不碰 DB，可安全放在鎖外）。
pub(crate) fn catalog_fetch_http(cfg: &CatalogCfg) -> Result<Vec<String>, String> {
    let url = catalog_url(&cfg.base, &cfg.format);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    if cfg.scheme == "goog-key" {
        req = req.header("x-goog-api-key", &cfg.key);
    } else if cfg.scheme == "anthropic" {
        req = req.header("x-api-key", &cfg.key);
        req = req.header("anthropic-version", "2023-06-01");
    } else {
        req = req.bearer_auth(&cfg.key);
    }
    let resp = req.send().map_err(|e| format!("連接上游失敗：{e}"))?;
    if !resp.status().is_success() {
        return Err(http_err(resp));
    }
    let v: serde_json::Value = resp.json().map_err(|e| format!("解析回包失敗：{e}"))?;
    parse_catalog(&v, &cfg.format)
}

/// 短鎖段：寫目錄快取（不碰網路）。
pub(crate) fn catalog_write(
    conn: &Connection,
    provider_id: i64,
    ids: Vec<String>,
) -> Result<CatalogCache, String> {
    let now = crate::fsutil::now_ms();
    conn.execute(
        "INSERT INTO model_catalogs (provider_id, fetched_at, models_json) VALUES (?1, ?2, ?3)
         ON CONFLICT(provider_id) DO UPDATE SET fetched_at=excluded.fetched_at, models_json=excluded.models_json",
        params![
            provider_id,
            now,
            serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into())
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(CatalogCache {
        provider_id,
        fetched_at: now,
        models: ids.into_iter().map(|id| CatalogEntry { id }).collect(),
    })
}

/// 相容入口（測試用）：讀 → 網路 → 寫。
#[cfg(test)]
pub(crate) fn catalog_fetch(
    conn: &Connection,
    provider_id: i64,
    api_key_override: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<CatalogCache, String> {
    let cfg = catalog_read_cfg(conn, provider_id, api_key_override, base_url_override)?;
    let ids = catalog_fetch_http(&cfg)?;
    catalog_write(conn, provider_id, ids)
}

// ---------------------------------------------------------------- CRUD ---

fn row_to_model(r: &Row) -> rusqlite::Result<UsableModel> {
    Ok(UsableModel {
        id: r.get(0)?,
        provider_id: r.get(1)?,
        display_name: r.get(2)?,
        actual_model: r.get(3)?,
        context_window: r.get(4)?,
        reasoning: r.get(5)?,
        ord: r.get(6)?,
        enabled: r.get::<_, i64>(7)? != 0,
    })
}

pub(crate) fn models_list(
    conn: &Connection,
    provider_id: i64,
) -> rusqlite::Result<Vec<UsableModel>> {
    let mut stmt = conn.prepare(
        "SELECT id, provider_id, display_name, actual_model, context_window, reasoning, ord, enabled
         FROM provider_models WHERE provider_id = ?1 ORDER BY ord ASC, id ASC",
    )?;
    let rows: Vec<UsableModel> = stmt
        .query_map([provider_id], row_to_model)?
        .collect::<rusqlite::Result<Vec<UsableModel>>>()?;
    Ok(rows)
}

pub(crate) fn model_counts(
    conn: &Connection,
) -> rusqlite::Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, COUNT(*) FROM provider_models GROUP BY provider_id",
    )?;
    let rows: Vec<(i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()?;
    Ok(rows)
}

fn validate_model(input: &UsableModelInput) -> Result<(), String> {
    if input.display_name.trim().is_empty() {
        return Err("選單顯示名不能為空".to_string());
    }
    if input.actual_model.trim().is_empty() {
        return Err("實際請求模型不能為空".to_string());
    }
    if let Some(cw) = input.context_window {
        if cw <= 0 {
            return Err("上下文視窗必須 > 0（不清請留空）".to_string());
        }
    }
    let reasoning_str = input.reasoning.trim();
    if reasoning_str.is_empty() || reasoning_str == "unset" {
        // 空或 unset 均合法（表示不指定）
    } else {
        // 支援逗號分隔的多選推理等級
        for part in reasoning_str.split(',') {
            let part = part.trim();
            if !part.is_empty() && !REASONINGS.contains(&part) {
                return Err(format!(
                    "思考等級「{part}」不合法，允許值：{}",
                    REASONINGS.join(" / ")
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn model_add(
    conn: &Connection,
    input: &UsableModelInput,
) -> Result<i64, String> {
    validate_model(input)?;
    conn.execute(
        "INSERT INTO provider_models
         (provider_id, display_name, actual_model, context_window, reasoning, ord, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            input.provider_id,
            input.display_name.trim(),
            input.actual_model.trim(),
            input.context_window,
            input.reasoning,
            input.ord,
            if input.enabled { 1 } else { 0 },
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "該渠道下顯示名已存在".to_string()
        } else {
            e.to_string()
        }
    })?;
    Ok(conn.last_insert_rowid())
}

pub(crate) fn model_update(
    conn: &Connection,
    id: i64,
    input: &UsableModelInput,
) -> Result<(), String> {
    validate_model(input)?;
    let n = conn
        .execute(
            "UPDATE provider_models SET display_name=?1, actual_model=?2, context_window=?3,
             reasoning=?4, ord=?5, enabled=?6 WHERE id=?7",
            params![
                input.display_name.trim(),
                input.actual_model.trim(),
                input.context_window,
                input.reasoning,
                input.ord,
                if input.enabled { 1 } else { 0 },
                id,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "該渠道下顯示名已存在".to_string()
            } else {
                e.to_string()
            }
        })?;
    if n == 0 {
        return Err(format!("模型不存在：{id}"));
    }
    Ok(())
}

pub(crate) fn model_delete(conn: &Connection, id: i64) -> Result<(), String> {
    let n = conn
        .execute("DELETE FROM provider_models WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("模型不存在：{id}"));
    }
    Ok(())
}

/// 翻譯用模型解析：display_name 優先命中，其次 actual 本身；都沒有則原樣透傳
///（上游報錯會原樣返回，便於排查映射缺失）。
pub(crate) fn resolve_actual(
    conn: &Connection,
    provider_id: i64,
    incoming: &str,
) -> String {
    let t = incoming.trim();
    if t.is_empty() {
        return String::new();
    }
    let hit: rusqlite::Result<String> = conn.query_row(
        "SELECT actual_model FROM provider_models
         WHERE provider_id = ?1 AND (display_name = ?2 OR actual_model = ?2)
         ORDER BY CASE WHEN display_name = ?2 THEN 0 ELSE 1 END LIMIT 1",
        params![provider_id, t],
        |r| r.get(0),
    );
    hit.unwrap_or_else(|_| t.to_string())
}

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
fn ensure_template(home: &Path, app_data: &Path) -> Result<PathBuf, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_both_shapes() {
        let openai: serde_json::Value = serde_json::from_str(
            r#"{"object":"list","data":[{"id":"a"},{"id":"b"},{"id":"a"},{"id":""}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_catalog(&openai, "openai-chat").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        let gem: serde_json::Value = serde_json::from_str(
            r#"{"models":[{"name":"models/gemini-3.6-flash"},{"name":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_catalog(&gem, "gemini").unwrap(),
            vec!["gemini-3.6-flash".to_string(), "x".to_string()]
        );
        assert!(parse_catalog(&openai, "openai-chat").is_ok());
        let bad: serde_json::Value = serde_json::from_str(r#"{"foo":[]}"#).unwrap();
        assert!(parse_catalog(&bad, "openai-chat").is_err());
    }

    #[test]
    fn empty_key_fails_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 種子渠道 key 為空：應直接提示填寫，不發請求
        let err = catalog_fetch(&conn, pid, None, None).unwrap_err();
        assert!(
            err.contains("API Key"),
            "應提示填寫 Key，實際：{err}"
        );
    }

    #[test]
    fn resolve_actual_prefers_display() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "claude-sonnet-4-6".into(),
                actual_model: "deepseek-v4-flash".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(
            resolve_actual(&conn, pid, "claude-sonnet-4-6"),
            "deepseek-v4-flash"
        );
        // actual 本身也命中
        assert_eq!(
            resolve_actual(&conn, pid, "deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
        // 無映射原樣透傳
        assert_eq!(resolve_actual(&conn, pid, "gpt-9"), "gpt-9");
        assert_eq!(resolve_actual(&conn, pid, "  "), "");
    }

    #[test]
    fn crud_validation() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='flatkey'", [], |r| {
                r.get(0)
            })
            .unwrap();
        // 種子 models_json 已遷移為行
        let rows = models_list(&conn, pid).unwrap();
        assert!(rows.iter().any(|r| r.actual_model == "deepseek-v4-flash"));
        let base = UsableModelInput {
            provider_id: pid,
            display_name: "flash".into(),
            actual_model: "deepseek-v4-flash".into(),
            context_window: Some(128000),
            reasoning: "high".into(),
            ord: 0,
            enabled: true,
        };
        let id = model_add(&conn, &base).unwrap();
        // 顯示名重複被拒
        assert!(model_add(&conn, &base).is_err());
        let mut bad = base.clone();
        bad.reasoning = "turbo".into();
        assert!(model_add(&conn, &bad).is_err());
        bad = base.clone();
        bad.context_window = Some(-1);
        assert!(model_add(&conn, &bad).is_err());
        // 逗號分隔多選推理等級
        let mut multi = base.clone();
        multi.display_name = "multi-reason".into();
        multi.reasoning = "low,medium,high".into();
        let multi_id = model_add(&conn, &multi).unwrap();
        let saved = models_list(&conn, pid)
            .unwrap()
            .into_iter()
            .find(|m| m.id == multi_id)
            .unwrap();
        assert_eq!(saved.reasoning, "low,medium,high");
        // Max 高思考等級可選
        let mut hi = base.clone();
        hi.display_name = "hi-reason".into();
        hi.reasoning = "max".into();
        let hi_id = model_add(&conn, &hi).unwrap();
        model_delete(&conn, hi_id).unwrap();
        // 含無效值的多選被拒
        let mut bad_multi = base.clone();
        bad_multi.display_name = "bad-multi".into();
        bad_multi.reasoning = "low,turbo".into();
        assert!(model_add(&conn, &bad_multi).is_err());
        model_delete(&conn, multi_id).unwrap();
        // 更新 + 刪除
        let mut upd = base.clone();
        upd.display_name = "flash2".into();
        model_update(&conn, id, &upd).unwrap();
        assert!(model_update(&conn, 999999, &upd).is_err());
        model_delete(&conn, id).unwrap();
        assert!(model_delete(&conn, id).is_err());
    }

    #[test]
    fn template_clone_generation() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        // 專用渠道（避開種子遷移行的干擾）
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
             models_json, priority, enabled, created_at, updated_at)
             VALUES ('t6','universal','openai-chat','http://x','', 'bearer','[]',0,1,0,0)",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='t6'", [], |r| r.get(0))
            .unwrap();
        // 自造模板（僅本單測用；真機用用户現有 catalog）
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": [{"slug": "tpl", "display_name": "tpl", "description": "tpl",
                "context_window": 1, "max_context_window": 1, "default_reasoning_level": "low",
                "supports_parallel_tool_calls": true}]}"#,
        )
        .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "我的 flash".into(),
                actual_model: "deepseek-v4-flash".into(),
                context_window: Some(200000),
                reasoning: "high".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        // 禁用的不進入目錄
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "關閉的".into(),
                actual_model: "x".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 10,
                enabled: false,
            },
        )
        .unwrap();
        // 多選推理等級：取規範序第一個作為 Codex 預設
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "multi-reason".into(),
                actual_model: "multi-model".into(),
                context_window: Some(100000),
                reasoning: "minimal,low,medium".into(),
                ord: 5,
                enabled: true,
            },
        )
        .unwrap();
        // 未指定等級 = 全部預設檔位
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "全部檔位".into(),
                actual_model: "all-model".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 3,
                enabled: true,
            },
        )
        .unwrap();
        let dest =
            codex_catalog_generate(&conn, &home, &dir.path().join("data"), pid, true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        let arr = v["models"].as_array().unwrap();
        // 聯合目錄：當前來源 3 個 + 種子渠道啟用模型（zen 等）
        assert!(arr.len() > 3, "應包含其他啟用來源的模型");
        assert!(
            arr.iter().any(|m| m["slug"] == "muse-spark-1.3-contributor-free"),
            "種子 zen 的模型應進入聯合目錄"
        );
        // slug 去重：flatkey 種子也有 deepseek-v4-flash，當前來源優先
        assert_eq!(
            arr.iter().filter(|m| m["slug"] == "deepseek-v4-flash").count(),
            1
        );
        let high_model = arr.iter().find(|m| m["slug"] == "deepseek-v4-flash").unwrap();
        assert_eq!(high_model["display_name"], "我的 flash");
        assert_eq!(high_model["context_window"], 200000);
        assert_eq!(high_model["max_context_window"], 200000);
        assert_eq!(high_model["default_reasoning_level"], "high");
        let efforts = |m: &serde_json::Value| -> Vec<String> {
            m["supported_reasoning_levels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|lv| lv["effort"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(efforts(high_model), vec!["high".to_string()]);
        let multi_model = arr.iter().find(|m| m["slug"] == "multi-model").unwrap();
        assert_eq!(multi_model["default_reasoning_level"], "minimal");
        assert_eq!(multi_model["context_window"], 100000);
        assert_eq!(
            efforts(multi_model),
            vec![
                "minimal".to_string(),
                "low".to_string(),
                "medium".to_string()
            ]
        );
        // unset = 模板並集；模板無檔位信息時退標準五檔，預設沿用模板值不覆寫
        let all_model = arr.iter().find(|m| m["slug"] == "all-model").unwrap();
        assert_eq!(all_model["default_reasoning_level"], "low");
        assert_eq!(
            efforts(all_model),
            vec![
                "minimal".to_string(),
                "low".to_string(),
                "medium".to_string(),
                "high".to_string(),
                "xhigh".to_string()
            ]
        );
        // 禁用模型不進目錄
        assert!(arr.iter().find(|m| m["slug"] == "x").is_none());
        // 模板其餘欄位原樣保留（版本安全）
        assert_eq!(arr[0]["supports_parallel_tool_calls"], true);
        assert_eq!(arr[0]["slug"], "deepseek-v4-flash");
        // issue #3668：桌面版認 `model` 欄位，必須與 slug 一致
        assert!(
            arr.iter().all(|m| m["model"] == m["slug"]),
            "每條目 model 必須等於 slug"
        );
        // 模板收編：legacy 模板應被複製為網關自持副本
        assert!(dir.path().join("data").join("catalogs").join("codex-template.json").exists());
    }

    #[test]
    fn catalog_single_source_excludes_others() {
        // 單來源模式：僅當前來源模型，不含其他啟用來源（選擇器不被污染）
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
             models_json, priority, enabled, created_at, updated_at)
             VALUES ('t7','universal','openai-chat','http://x','', 'bearer','[]',0,1,0,0)",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM providers WHERE name='t7'", [], |r| r.get(0))
            .unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": [{"slug": "tpl", "display_name": "tpl", "description": "tpl",
                "context_window": 1, "max_context_window": 1, "default_reasoning_level": "low",
                "supports_parallel_tool_calls": true}]}"#,
        )
        .unwrap();
        model_add(
            &conn,
            &UsableModelInput {
                provider_id: pid,
                display_name: "only".into(),
                actual_model: "only-model".into(),
                context_window: None,
                reasoning: "unset".into(),
                ord: 0,
                enabled: true,
            },
        )
        .unwrap();
        let dest =
            codex_catalog_generate(&conn, &home, &dir.path().join("data"), pid, false).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        let arr = v["models"].as_array().unwrap();
        assert_eq!(arr.len(), 1, "單來源只能有 1 條：{arr:?}");
        assert_eq!(arr[0]["slug"], "only-model");
        assert_eq!(arr[0]["model"], "only-model");
    }

    #[test]
    fn template_vendoring_prefers_gateway_copy() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("cc-switch-model-catalog.json"),
            r#"{"models": []}"#,
        )
        .unwrap();
        let data = dir.path().join("data");
        // 首次：從 legacy 收編
        let p1 = ensure_template(&home, &data).unwrap();
        assert_eq!(p1, data.join("catalogs").join("codex-template.json"));
        assert!(p1.exists());
        // 刪除 legacy 後仍可用網關自持副本（不再依賴 cc-switch）
        std::fs::remove_file(home.join(".codex").join("cc-switch-model-catalog.json")).unwrap();
        let p2 = ensure_template(&home, &data).unwrap();
        assert_eq!(p2, p1);
        // 兩邊都沒有 → 明確報錯
        std::fs::remove_file(&p1).unwrap();
        assert!(ensure_template(&home, &data).is_err());
    }
}
