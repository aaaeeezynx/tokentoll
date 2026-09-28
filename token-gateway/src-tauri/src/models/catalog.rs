//! 上游模型目錄抓取與快取（原 `models.rs` 的目錄抓取段）。

use rusqlite::{params, Connection};
use serde::Serialize;

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
