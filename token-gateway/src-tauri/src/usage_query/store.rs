//! 用量查詢設定的資料庫存取（`provider_usage_query` 一列對一份設定）。

use rusqlite::{params, Connection};

use super::{HeaderPair, UsageQueryConfig};
use crate::fsutil::now_ms;

fn row_to_config(r: &rusqlite::Row) -> rusqlite::Result<UsageQueryConfig> {
    let headers_json: String = r.get(9)?;
    let extractor_json: String = r.get(10)?;
    Ok(UsageQueryConfig {
        provider_id: r.get(0)?,
        enabled: r.get::<_, i64>(1)? != 0,
        template: r.get(2)?,
        base_url: r.get(3)?,
        api_key: r.get(4)?,
        access_token: r.get(5)?,
        user_id: r.get(6)?,
        url_template: r.get(7)?,
        method: r.get(8)?,
        headers: serde_json::from_str::<Vec<HeaderPair>>(&headers_json).unwrap_or_default(),
        extractor: serde_json::from_str(&extractor_json)
            .unwrap_or(serde_json::Value::Object(Default::default())),
        timeout_secs: r.get(11)?,
        auto_interval_min: r.get(12)?,
    })
}

const SELECT: &str = "SELECT provider_id, enabled, template, base_url, api_key, access_token,
                             user_id, url_template, method, headers_json, extractor_json,
                             timeout_secs, auto_interval_min
                      FROM provider_usage_query";

/// 讀一份設定（沒有就回 `None`：代表這個來源還沒設定過用量查詢）。
pub(crate) fn load(conn: &Connection, provider_id: i64) -> Option<UsageQueryConfig> {
    conn.query_row(&format!("{SELECT} WHERE provider_id=?1"), [provider_id], row_to_config)
        .ok()
}

/// 讀所有**已啟用**的設定（卡片顯示用）。
pub(crate) fn load_enabled(conn: &Connection) -> Vec<UsageQueryConfig> {
    let Ok(mut stmt) = conn.prepare(&format!("{SELECT} WHERE enabled=1 ORDER BY provider_id")) else {
        return vec![];
    };
    stmt.query_map([], row_to_config)
        .map(|it| it.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

/// 寫入（UPSERT）。
pub(crate) fn save(conn: &Connection, cfg: &UsageQueryConfig) -> Result<(), String> {
    if cfg.provider_id <= 0 {
        return Err("provider_id 無效".to_string());
    }
    if cfg.url_template.trim().is_empty() {
        return Err("查詢網址不能為空".to_string());
    }
    let method = cfg.method.trim().to_ascii_uppercase();
    if method != "GET" && method != "POST" {
        return Err(format!("只支援 GET／POST，收到 {method}"));
    }
    if !(1..=120).contains(&cfg.timeout_secs) {
        return Err("逾時請填 1–120 秒".to_string());
    }
    if !(0..=1440).contains(&cfg.auto_interval_min) {
        return Err("自動查詢間隔請填 0–1440 分（0 = 不自動查）".to_string());
    }
    conn.execute(
        "INSERT INTO provider_usage_query
           (provider_id, enabled, template, base_url, api_key, access_token, user_id,
            url_template, method, headers_json, extractor_json, timeout_secs,
            auto_interval_min, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
         ON CONFLICT(provider_id) DO UPDATE SET
           enabled=excluded.enabled, template=excluded.template, base_url=excluded.base_url,
           api_key=excluded.api_key, access_token=excluded.access_token, user_id=excluded.user_id,
           url_template=excluded.url_template, method=excluded.method,
           headers_json=excluded.headers_json, extractor_json=excluded.extractor_json,
           timeout_secs=excluded.timeout_secs,
           auto_interval_min=excluded.auto_interval_min, updated_at=excluded.updated_at",
        params![
            cfg.provider_id,
            if cfg.enabled { 1 } else { 0 },
            cfg.template,
            cfg.base_url.trim(),
            cfg.api_key,
            cfg.access_token,
            cfg.user_id,
            cfg.url_template.trim(),
            method,
            serde_json::to_string(&cfg.headers).unwrap_or_else(|_| "[]".into()),
            cfg.extractor.to_string(),
            cfg.timeout_secs,
            cfg.auto_interval_min,
            now_ms(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 刪除設定（來源刪除時一起清，避免孤兒列）。
pub(crate) fn delete(conn: &Connection, provider_id: i64) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM provider_usage_query WHERE provider_id=?1",
        [provider_id],
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_query::HeaderPair;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn sample(pid: i64) -> UsageQueryConfig {
        UsageQueryConfig {
            provider_id: pid,
            enabled: true,
            template: "newapi".into(),
            base_url: "https://relay.example.com".into(),
            api_key: "sk-x".into(),
            access_token: "tok".into(),
            user_id: "7".into(),
            url_template: "{{baseUrl}}/api/user/self".into(),
            method: "get".into(),
            headers: vec![HeaderPair {
                name: "Authorization".into(),
                value: "Bearer {{accessToken}}".into(),
            }],
            extractor: serde_json::json!({"remaining": "data.quota", "divide": 500000}),
            timeout_secs: 10,
            auto_interval_min: 30,
        }
    }

    #[test]
    fn roundtrip_and_upsert() {
        let (_d, conn) = db();
        assert!(load(&conn, 1).is_none(), "沒設定過要回 None");
        let mut cfg = sample(1);
        save(&conn, &cfg).unwrap();
        let got = load(&conn, 1).unwrap();
        assert_eq!(got.provider_id, 1);
        assert!(got.enabled);
        assert_eq!(got.template, "newapi");
        assert_eq!(got.method, "GET", "method 要正規化成大寫");
        assert_eq!(got.headers.len(), 1);
        assert_eq!(got.headers[0].value, "Bearer {{accessToken}}");
        assert_eq!(got.extractor["remaining"], "data.quota");
        assert_eq!(got.auto_interval_min, 30);

        // 再存一次＝更新（不是多一列）
        cfg.auto_interval_min = 0;
        cfg.enabled = false;
        save(&conn, &cfg).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider_usage_query", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "UPSERT 不該多一列");
        assert!(!load(&conn, 1).unwrap().enabled);
        assert!(load_enabled(&conn).is_empty(), "停用的不該出現在 enabled 清單");
    }

    #[test]
    fn validation_rejects_junk() {
        let (_d, conn) = db();
        let mut c = sample(1);
        c.url_template = "   ".into();
        assert!(save(&conn, &c).unwrap_err().contains("網址"));
        let mut c = sample(1);
        c.method = "DELETE".into();
        assert!(save(&conn, &c).unwrap_err().contains("只支援 GET／POST"));
        let mut c = sample(1);
        c.timeout_secs = 0;
        assert!(save(&conn, &c).unwrap_err().contains("逾時"));
        let mut c = sample(1);
        c.auto_interval_min = 5000;
        assert!(save(&conn, &c).unwrap_err().contains("0–1440"));
        let mut c = sample(1);
        c.provider_id = 0;
        assert!(save(&conn, &c).unwrap_err().contains("provider_id"));
        // 全部被擋下來 → 一列都沒有
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM provider_usage_query", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn delete_removes_the_row() {
        let (_d, conn) = db();
        save(&conn, &sample(3)).unwrap();
        assert_eq!(delete(&conn, 3).unwrap(), 1);
        assert!(load(&conn, 3).is_none());
        assert_eq!(delete(&conn, 3).unwrap(), 0, "再刪一次不該出錯");
    }
}
