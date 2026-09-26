//! M3：本地 Key（`sk-local-…`）簽發與鑑權。
//!
//! - 金鑰明文只在創建瞬間返回一次；庫裡只存 SHA-256 哈希 + 可展示前綴。
//! - 一個 Key 綁定一個上游渠道（`provider_id`），網關按此路由。
//! - `quota_tokens < 0` 表示不限量；`rate_limit_qpm <= 0` 表示不限流。

use rand::distr::{Alphanumeric, SampleString};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fsutil::now_ms;

pub const KEY_PREFIX: &str = "sk-local-";

pub fn sha256_hex(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

fn gen_secret() -> String {
    let rand_part = Alphanumeric.sample_string(&mut rand::rng(), 32);
    format!("{KEY_PREFIX}{rand_part}")
}

fn parse_json_array(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalKey {
    pub id: i64,
    pub name: String,
    /// 僅展示用前綴（如 `sk-local-Ab12…`），不可反推。
    pub key_prefix: String,
    pub quota_tokens: i64,
    pub used_tokens: i64,
    pub rate_limit_qpm: i64,
    pub allowed_models: Vec<String>,
    pub allowed_apps: Vec<String>,
    pub provider_id: Option<i64>,
    pub provider_name: Option<String>,
    pub expires_at: Option<i64>,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyInput {
    pub name: String,
    pub provider_id: Option<i64>,
    pub quota_tokens: i64,
    pub rate_limit_qpm: i64,
    pub allowed_models: Vec<String>,
    pub allowed_apps: Vec<String>,
    /// 過期毫秒時間戳；None = 永不過期。
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyCreated {
    pub id: i64,
    /// 明文，只返回這一次，前端必須提示用戶立即複製並儲存。
    pub secret: String,
}

fn row_to_key(row: &Row) -> rusqlite::Result<LocalKey> {
    let allowed_models: String = row.get(6)?;
    let allowed_apps: String = row.get(7)?;
    Ok(LocalKey {
        id: row.get(0)?,
        key_prefix: row.get(1)?,
        name: row.get(2)?,
        quota_tokens: row.get(3)?,
        used_tokens: row.get(4)?,
        rate_limit_qpm: row.get(5)?,
        allowed_models: parse_json_array(&allowed_models),
        allowed_apps: parse_json_array(&allowed_apps),
        provider_id: row.get(8)?,
        provider_name: row.get(9)?,
        expires_at: row.get(10)?,
        enabled: row.get::<_, i64>(11)? != 0,
        created_at: row.get(12)?,
    })
}

const SELECT: &str = "SELECT k.id, k.key_prefix, k.name, k.quota_tokens, k.used_tokens,
    k.rate_limit_qpm, k.allowed_models_json, k.allowed_apps_json, k.provider_id, p.name,
    k.expires_at, k.enabled, k.created_at
    FROM local_keys k LEFT JOIN providers p ON p.id = k.provider_id ORDER BY k.id ASC";

pub(crate) fn list_keys(conn: &Connection) -> rusqlite::Result<Vec<LocalKey>> {
    let mut stmt = conn.prepare(SELECT)?;
    let rows = stmt.query_map([], row_to_key)?;
    rows.collect()
}

pub(crate) fn create_key(
    conn: &Connection,
    input: &KeyInput,
) -> Result<KeyCreated, String> {
    if input.name.trim().is_empty() {
        return Err("Key 名稱不能為空".to_string());
    }
    if let Some(pid) = input.provider_id {
        let exists: bool = conn
            .query_row("SELECT 1 FROM providers WHERE id=?1", [pid], |_| Ok(()))
            .is_ok();
        if !exists {
            return Err(format!("綁定的渠道不存在：{pid}"));
        }
    }
    let secret = gen_secret();
    let hash = sha256_hex(&secret);
    let prefix = format!("{}…{}", &secret[..12], &secret[secret.len() - 4..]);
    conn.execute(
        "INSERT INTO local_keys (key_prefix, key_hash, key_plain, name, quota_tokens, used_tokens,
         rate_limit_qpm, allowed_models_json, allowed_apps_json, provider_id, expires_at, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10, 1, ?11)",
        params![
            prefix,
            hash,
            secret,
            input.name.trim(),
            input.quota_tokens,
            input.rate_limit_qpm,
            serde_json::to_string(&input.allowed_models).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&input.allowed_apps).unwrap_or_else(|_| "[]".into()),
            input.provider_id,
            input.expires_at,
            now_ms(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(KeyCreated {
        id: conn.last_insert_rowid(),
        secret,
    })
}

pub(crate) fn update_key(
    conn: &Connection,
    id: i64,
    input: &KeyInput,
) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("Key 名稱不能為空".to_string());
    }
    let n = conn
        .execute(
            "UPDATE local_keys SET name=?1, quota_tokens=?2, rate_limit_qpm=?3,
             allowed_models_json=?4, allowed_apps_json=?5, provider_id=?6, expires_at=?7 WHERE id=?8",
            params![
                input.name.trim(),
                input.quota_tokens,
                input.rate_limit_qpm,
                serde_json::to_string(&input.allowed_models).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&input.allowed_apps).unwrap_or_else(|_| "[]".into()),
                input.provider_id,
                input.expires_at,
                id,
            ],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("Key 不存在：{id}"));
    }
    Ok(())
}

pub(crate) fn set_key_enabled(
    conn: &Connection,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE local_keys SET enabled=?1 WHERE id=?2",
            params![if enabled { 1 } else { 0 }, id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("Key 不存在：{id}"));
    }
    Ok(())
}

pub(crate) fn delete_key(conn: &Connection, id: i64) -> Result<(), String> {
    let n = conn
        .execute("DELETE FROM local_keys WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("Key 不存在：{id}"));
    }
    Ok(())
}

/// 鑑權結果：通過則返回可用 Key 行（含上游渠道資訊）。
#[derive(Debug, Clone)]
pub(crate) struct AuthedKey {
    /// 本地 Key id；直連上游模式為 -1（無本地 Key，不計本地 Key 配額，用量仍記 request_logs）。
    pub id: i64,
    pub rate_limit_qpm: i64,
    pub allowed_models: Vec<String>,
    pub allowed_apps: Vec<String>,
    pub provider_id: i64,
    pub provider_base_url: String,
    pub provider_api_key: String,
    /// 上游鑑權方式：bearer | goog-key | anthropic。
    pub provider_auth_scheme: String,
    /// 上游格式：anthropic | openai-chat | openai-responses | gemini | mixed。
    pub provider_api_format: String,
    /// 直連上游模式：Bearer 即上游渠道自身的 api_key，不經本地 Key 轉發；
    /// 僅允許該渠道、不做跨來源路由；用量照常記錄（僅不計本地 Key 配額）。
    pub direct: bool,
}

/// auth_key 查詢行的列類型（id, quota, used, qpm, models, apps, pid, base, key, scheme, format, exp, enabled）。
type AuthRow = (
    i64,
    i64,
    i64,
    i64,
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
);

pub(crate) fn auth_key(conn: &Connection, secret: &str) -> Result<AuthedKey, (u16, String)> {
    let hash = sha256_hex(secret);
    let row: rusqlite::Result<AuthRow> = conn.query_row(
        "SELECT k.id, k.quota_tokens, k.used_tokens, k.rate_limit_qpm,
                k.allowed_models_json, k.allowed_apps_json, k.provider_id,
                p.base_url, p.api_key, p.auth_scheme, p.api_format, k.expires_at, k.enabled
         FROM local_keys k LEFT JOIN providers p ON p.id = k.provider_id
         WHERE k.key_hash = ?1",
        [&hash],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
            ))
        },
    );
    let (id, quota, used, qpm, models_json, apps_json, pid, base_url, api_key, scheme, format, expires, enabled) =
        row.map_err(|_| (401u16, "無效的本地 Key".to_string()))?;
    if enabled == 0 {
        return Err((401, "該 Key 已被停用".to_string()));
    }
    if let Some(exp) = expires {
        if now_ms() > exp {
            return Err((401, "該 Key 已過期".to_string()));
        }
    }
    if quota >= 0 && used >= quota {
        return Err((429, "該 Key 配額已用完".to_string()));
    }
    let pid = pid.ok_or((500, "該 Key 未綁定上游渠道".to_string()))?;
    let base_url: String = base_url.ok_or((500, "綁定的上游渠道不存在".to_string()))?;
    Ok(AuthedKey {
        id,
        rate_limit_qpm: qpm,
        allowed_models: parse_json_array(&models_json),
        allowed_apps: parse_json_array(&apps_json),
        provider_id: pid,
        provider_base_url: base_url,
        provider_api_key: api_key.unwrap_or_default(),
        provider_auth_scheme: scheme.unwrap_or_else(|| "bearer".to_string()),
        provider_api_format: format.unwrap_or_else(|| "openai-chat".to_string()),
        direct: false,
    })
}

/// 直連上游鑑權：Bearer 即某啟用渠道自身的上游 api_key（非空、精確匹配）時，
/// 視為該渠道的直連請求，不經本地 Key 轉發、僅允許該渠道；
/// 用量照常記入 request_logs（tokens/cost/provider 歸因，僅不計本地 Key 配額）。
/// 本地 Key 優先：先走 [`auth_key`]，401 才回退到此。
pub(crate) fn auth_direct(conn: &Connection, secret: &str) -> Result<AuthedKey, (u16, String)> {
    if secret.trim().is_empty() {
        return Err((401, "無效的 Key".to_string()));
    }
    let row: rusqlite::Result<(i64, String, String, String, String)> = conn.query_row(
        "SELECT id, base_url, api_key, auth_scheme, api_format FROM providers
         WHERE enabled = 1 AND api_key IS NOT NULL AND api_key <> '' AND api_key = ?1",
        [secret],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    );
    let (pid, base_url, api_key, scheme, format) =
        row.map_err(|_| (401u16, "無效的 Key（既非本地 Key，也非已登記渠道的上游 Key）".to_string()))?;
    Ok(AuthedKey {
        id: -1,
        rate_limit_qpm: 0,
        allowed_models: vec![],
        allowed_apps: vec![],
        provider_id: pid,
        provider_base_url: base_url,
        provider_api_key: api_key,
        provider_auth_scheme: scheme,
        provider_api_format: format,
        direct: true,
    })
}

/// 輪換：重新生成明文並替換哈希（舊 Key 立即失效，只返回一次新明文）。
pub(crate) fn rotate_key(conn: &Connection, id: i64) -> Result<KeyCreated, String> {
    let exists: bool = conn
        .query_row("SELECT 1 FROM local_keys WHERE id=?1", [id], |_| Ok(()))
        .is_ok();
    if !exists {
        return Err(format!("Key 不存在：{id}"));
    }
    let secret = gen_secret();
    let hash = sha256_hex(&secret);
    let prefix = format!("{}…{}", &secret[..12], &secret[secret.len() - 4..]);
    conn.execute(
        "UPDATE local_keys SET key_hash=?1, key_prefix=?2, key_plain=?3 WHERE id=?4",
        params![hash, prefix, secret, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(KeyCreated { id, secret })
}

pub(crate) fn add_used(conn: &Connection, id: i64, tokens: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE local_keys SET used_tokens = used_tokens + ?1 WHERE id = ?2",
        params![tokens, id],
    )?;
    Ok(())
}

pub(crate) fn reveal_key(conn: &Connection, id: i64) -> Result<String, String> {
    let plain: String = conn
        .query_row(
            "SELECT key_plain FROM local_keys WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| format!("Key 不存在：{id}"))?;
    if plain.trim().is_empty() {
        return Err("該 Key 為舊版本簽發，無法找回明文，請使用輪換生成新 Key".to_string());
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_roundtrip_and_prefix() {
        let s1 = gen_secret();
        let s2 = gen_secret();
        assert!(s1.starts_with(KEY_PREFIX));
        assert_ne!(s1, s2);
        assert_ne!(sha256_hex(&s1), s1, "存的必須是哈希");
        assert_eq!(sha256_hex(&s1).len(), 64);
    }

    fn test_input() -> KeyInput {
        KeyInput {
            name: "k".into(),
            provider_id: None,
            quota_tokens: -1,
            rate_limit_qpm: 0,
            allowed_models: vec![],
            allowed_apps: vec![],
            expires_at: None,
        }
    }

    #[test]
    fn direct_matches_provider_key() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
             models_json, priority, enabled, created_at, updated_at)
             VALUES ('d1','universal','openai-chat','http://up','up-secret','bearer','[]',0,1,0,0)",
            [],
        )
        .unwrap();
        let a = auth_direct(&conn, "up-secret").unwrap();
        assert!(a.direct);
        assert_eq!(a.id, -1);
        assert_eq!(a.provider_api_key, "up-secret");
        assert_eq!(a.provider_base_url, "http://up");
        assert!(auth_direct(&conn, "nope").is_err());
        assert!(auth_direct(&conn, "").is_err());
        assert!(auth_direct(&conn, "   ").is_err());
    }

    #[test]
    fn rotate_replaces_secret() {        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let created = create_key(&conn, &test_input()).unwrap();
        assert!(auth_key(&conn, &created.secret).is_err(), "未綁定渠道應 500");
        let r2 = rotate_key(&conn, created.id).unwrap();
        assert_eq!(r2.id, created.id);
        assert_ne!(r2.secret, created.secret);
        assert!(auth_key(&conn, &created.secret).is_err(), "舊 Key 必須失效");
        assert!(rotate_key(&conn, 999999).is_err());
    }
}
