//! 可用模型 CRUD、思考等級白名單與實際模型解析（原 `models.rs` 的「CRUD」段）。

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

/// 思考等級（七檔；unset = 不指定，目錄使用全部預設檔位）。
pub const REASONINGS: [&str; 7] = [
    "unset", "minimal", "low", "medium", "high", "xhigh", "max",
];

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
