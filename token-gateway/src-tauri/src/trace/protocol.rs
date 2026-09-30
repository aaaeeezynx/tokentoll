//! 學到的上游協議記憶（`provider_model_protocol`）。
//!
//! 為什麼要落庫：同一個來源的模型可能**逐模型**只在一種端點上架（2026-09-30 對
//! opencode-go 直接量測：`grok-4.7` 只在 `/responses`、`mimo-v2.6-*` 只在
//! `/chat/completions`）。網關先用宣告協議打，被回「此模型不支援本協議」就換另一種
//! 協議重送。**只存在記憶體的話，網關每次重啟，每個這種模型都要多付一次失敗探測**
//! —— 與 `provider_stripped_fields` 當年的問題一模一樣，所以用同一種做法解決。
//!
//! `declared_format` 放進主鍵是刻意的：使用者把來源協議從 `openai-chat` 改成
//! `openai-responses` 是明確的設定變更，不可以被舊的學習結果蓋掉。

use rusqlite::Connection;

use crate::fsutil::now_ms;

// ───────────────────────────────────── 上游協議的持久化記憶 ───

/// 記住某來源的某模型「在宣告協議 X 之下，實際用 Y 會通」（冪等，後蓋前）。
pub fn remember_protocol(
    conn: &Connection,
    provider_id: i64,
    model: &str,
    declared_format: &str,
    actual_format: &str,
) -> Result<(), String> {
    let model = model.trim().to_lowercase();
    if model.is_empty() || actual_format.trim().is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO provider_model_protocol
             (provider_id, model, declared_format, actual_format, learned_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(provider_id, model, declared_format)
         DO UPDATE SET actual_format = excluded.actual_format, learned_at = excluded.learned_at",
        rusqlite::params![
            provider_id,
            model,
            declared_format.trim().to_lowercase(),
            actual_format.trim().to_lowercase(),
            now_ms()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 讀取某來源的某模型在「宣告協議 `declared_format`」之下學到的實際協議。
pub fn load_protocol(
    conn: &Connection,
    provider_id: i64,
    model: &str,
    declared_format: &str,
) -> Option<String> {
    let model = model.trim().to_lowercase();
    if model.is_empty() {
        return None;
    }
    conn.query_row(
        "SELECT actual_format FROM provider_model_protocol
         WHERE provider_id = ?1 AND model = ?2 AND declared_format = ?3",
        rusqlite::params![provider_id, model, declared_format.trim().to_lowercase()],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// 清除某來源學到的協議記憶（與「重設上游能力記憶」一起用）。
pub fn clear_protocol(conn: &Connection, provider_id: i64) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM provider_model_protocol WHERE provider_id = ?1",
        [provider_id],
    )
    .map_err(|e| e.to_string())
}

/// 學到的協議總覽（診斷頁用）：（來源 id、來源名、模型、宣告協議、實際協議）。
pub fn protocol_summary(conn: &Connection) -> Vec<(i64, String, String, String, String)> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT p.id, COALESCE(p.name,''), m.model, m.declared_format, m.actual_format
         FROM provider_model_protocol m
         LEFT JOIN providers p ON p.id = m.provider_id
         ORDER BY p.priority ASC, p.id ASC, m.model ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    }) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}
