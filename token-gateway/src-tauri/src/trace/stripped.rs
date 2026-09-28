//! 上游拒收欄位的持久化記憶（`provider_stripped_fields`）。

use rusqlite::Connection;

use crate::fsutil::now_ms;

/// 記住某渠道被上游拒收的欄位（冪等）。
pub fn remember_stripped(conn: &Connection, provider_id: i64, field: &str) -> Result<(), String> {
    if field.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO provider_stripped_fields (provider_id, field, learned_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(provider_id, field) DO NOTHING",
        rusqlite::params![provider_id, field, now_ms()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 讀取某渠道已知拒收的欄位（依學習時間排序，穩定可重現）。
pub fn load_stripped(conn: &Connection, provider_id: i64) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT field FROM provider_stripped_fields
         WHERE provider_id = ?1 ORDER BY learned_at ASC, field ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([provider_id], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// 清除某渠道的拒收記憶（UI 的「重設上游能力記憶」用）。
pub fn clear_stripped(conn: &Connection, provider_id: i64) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM provider_stripped_fields WHERE provider_id = ?1",
        [provider_id],
    )
    .map_err(|e| e.to_string())
}

/// 掃描出所有「某欄位被多個渠道拒收」的欄位名（診斷頁用）。
pub fn stripped_field_summary(conn: &Connection) -> Vec<(String, i64)> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT field, COUNT(DISTINCT provider_id) AS n
         FROM provider_stripped_fields GROUP BY field ORDER BY n DESC, field ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}
