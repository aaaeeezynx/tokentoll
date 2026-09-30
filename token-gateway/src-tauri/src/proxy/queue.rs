//! 故障轉移佇列的**唯讀預覽**（P1.4：對標 CC Switch 的 failover queue 畫面）。
//!
//! 這裡不重寫任何排序邏輯 —— 它呼叫的 `resolve_model_providers` 就是
//! `proxy_handler` 在換手時用的**同一個查詢**，所以畫面上看到的順序就是實際會走的
//! 順序。這一點是刻意的：一旦有第二份排序邏輯，畫面就會開始說謊。
//!
//! 預覽是「以模型為中心」的（使用者選一個模型 → 看它在哪些來源上架、順序如何）。
//! 實際請求還會多一層「協議」（宣告協議／學到的協議／另一種協議），那部分在
//! `failover.rs` 決定，並且逐模型記錄在 `provider_model_protocol`（這裡一併顯示
//! 「學到的協議」讓使用者看得出網關記住了什麼）。

use rusqlite::Connection;

use super::*;

/// 佇列裡的一列（一個來源）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueueRow {
    pub provider_id: i64,
    pub provider_name: String,
    pub api_format: String,
    /// 來源是否啟用（停用的來源不會進候選，這裡仍列出並標示，免得使用者以為漏了）
    pub enabled: bool,
    /// 這個來源總共登記了幾個（啟用的）模型
    pub model_count: i64,
    /// 這個來源對「這個模型」學到的協議（沒有就 null）
    pub learned_format: Option<String>,
    /// 斷路器：目前是否冷卻中（冷卻中的會被排到候選最後）
    pub open: bool,
    pub cooldown_secs: u64,
}

/// 佇列模型清單（挑模型用）：模型 + 有幾個來源登記它。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueueModel {
    pub model: String,
    pub sources: i64,
}

/// 列出「至少有一個啟用來源登記」的模型；來源多的排前面（那些才是真的有備援的）。
pub fn queue_models(conn: &Connection) -> Result<Vec<QueueModel>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(NULLIF(m.actual_model, ''), m.display_name) AS model,
                    COUNT(DISTINCT m.provider_id) AS sources
             FROM provider_models m JOIN providers p ON p.id = m.provider_id
             WHERE m.enabled = 1 AND p.enabled = 1
             GROUP BY lower(model)
             ORDER BY sources DESC, model ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(QueueModel {
                model: r.get(0)?,
                sources: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// 某個模型的候選來源順序（＝實際換手順序）。
pub fn queue_rows(
    conn: &Connection,
    db_path: &std::path::Path,
    model: &str,
    health: &HealthRegistry,
) -> Result<Vec<QueueRow>, String> {
    // exclude_pid = -1：預覽不綁 Key，所以「不排除任何來源」。
    let provs = resolve_model_providers(conn, model, -1);
    // 冷卻秒數只在 snapshot 裡，這裡抓一次做成表（來源數量很少）。
    let cooling: HashMap<i64, u64> = health
        .snapshot()
        .into_iter()
        .map(|h| (h.provider_id, h.cooldown_secs))
        .collect();
    let mut out = Vec::with_capacity(provs.len());
    for p in provs {
        let (provider_name, enabled) = conn
            .query_row(
                "SELECT name, enabled FROM providers WHERE id=?1",
                [p.id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .unwrap_or_else(|_| (String::new(), false));
        let model_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_models WHERE provider_id=?1 AND enabled=1",
                [p.id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        out.push(QueueRow {
            provider_id: p.id,
            provider_name,
            learned_format: learned_format(db_path, p.id, model, &p.format),
            api_format: p.format,
            enabled,
            model_count,
            open: health.is_open(p.id),
            cooldown_secs: cooling.get(&p.id).copied().unwrap_or(0),
        });
    }
    // 冷卻中的排最後 —— 與 `order_by_health` 在請求路徑上的行為一致，
    // 否則畫面顯示的順序會與實際嘗試的順序不同。
    Ok(order_by_health_rows(out, health))
}

/// 把冷卻中的來源排到最後（穩定排序），與 `health::order_by_health` 同一條規則。
fn order_by_health_rows(rows: Vec<QueueRow>, health: &HealthRegistry) -> Vec<QueueRow> {
    let (mut ok, mut cooling): (Vec<QueueRow>, Vec<QueueRow>) = (Vec::new(), Vec::new());
    for r in rows {
        if health.is_open(r.provider_id) {
            cooling.push(r);
        } else {
            ok.push(r);
        }
    }
    if cooling.is_empty() {
        return ok;
    }
    ok.extend(cooling);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_provider(conn: &Connection, name: &str, fmt: &str, enabled: i64) -> i64 {
        let pr: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(priority), -1) + 1 FROM providers",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO providers (name, app_type, api_format, base_url, api_key, auth_scheme,
                                    models_json, priority, enabled, created_at, updated_at)
             VALUES (?1,'universal',?2,'https://example.com/v1','k','bearer','[]',?3,?4,0,0)",
            rusqlite::params![name, fmt, pr, enabled],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn add_model(conn: &Connection, pid: i64, name: &str, enabled: i64) {
        conn.execute(
            "INSERT INTO provider_models (provider_id, display_name, actual_model, ord, enabled)
             VALUES (?1,?2,?2,0,?3)",
            rusqlite::params![pid, name, enabled],
        )
        .unwrap();
    }

    /// 測試用的空庫：回傳 (臨時目錄, 資料庫檔路徑, 連線)。
    /// 路徑要留著 —— `queue_rows` 需要它讀「學到的協議」，傳目錄會靜默讀不到。
    fn db() -> (tempfile::TempDir, std::path::PathBuf, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("q.db");
        let conn = crate::db::open_and_ensure(&path).unwrap();
        conn.execute("DELETE FROM provider_models", []).unwrap();
        conn.execute("DELETE FROM providers", []).unwrap();
        (dir, path, conn)
    }

    /// 佇列順序＝來源清單順序＝`resolve_model_providers` 的順序。
    #[test]
    fn queue_order_follows_provider_priority() {
        let (_d, path, conn) = db();
        let a = insert_provider(&conn, "first", "openai-chat", 1);
        let b = insert_provider(&conn, "second", "openai-responses", 1);
        add_model(&conn, a, "m", 1);
        add_model(&conn, b, "m", 1);
        let health = HealthRegistry::with_policy(3, std::time::Duration::from_secs(60));
        let rows = queue_rows(&conn, &path, "m", &health).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.provider_name.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(rows[0].api_format, "openai-chat");
        assert_eq!(rows[1].api_format, "openai-responses");
        assert!(rows.iter().all(|r| r.enabled && r.model_count == 1));
    }

    /// 停用的來源與停用的模型都不進候選（與請求路徑一致）。
    #[test]
    fn disabled_sources_and_models_are_excluded() {
        let (_d, path, conn) = db();
        let on = insert_provider(&conn, "on", "openai-chat", 1);
        let off = insert_provider(&conn, "off", "openai-chat", 0);
        add_model(&conn, on, "m", 1);
        add_model(&conn, off, "m", 1);
        add_model(&conn, on, "disabled-model", 0);
        let health = HealthRegistry::default();
        let rows = queue_rows(&conn, &path, "m", &health).unwrap();
        assert_eq!(rows.len(), 1, "停用的來源不該進候選");
        assert_eq!(rows[0].provider_name, "on");
        assert!(queue_rows(&conn, &path, "disabled-model", &health)
            .unwrap()
            .is_empty());
    }

    /// 冷卻中的來源在畫面上也要被排到最後（否則畫面與實際行為不一致）。
    #[test]
    fn cooling_source_is_listed_last() {
        let (_d, path, conn) = db();
        let a = insert_provider(&conn, "first", "openai-chat", 1);
        let b = insert_provider(&conn, "second", "openai-chat", 1);
        add_model(&conn, a, "m", 1);
        add_model(&conn, b, "m", 1);
        let health = HealthRegistry::with_policy(2, std::time::Duration::from_secs(60));
        health.record_failure(a, "boom");
        health.record_failure(a, "boom");
        assert!(health.is_open(a));
        let rows = queue_rows(&conn, &path, "m", &health).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.provider_name.as_str())
                .collect::<Vec<_>>(),
            vec!["second", "first"]
        );
        assert!(rows[1].open && rows[1].cooldown_secs > 0);
    }

    /// 學到的協議要顯示出來（讓使用者看得出網關記住了什麼）。
    #[test]
    fn learned_format_is_shown() {
        let (_d, path, conn) = db();
        let a = insert_provider(&conn, "oc", "openai-chat", 1);
        add_model(&conn, a, "grok-x", 1);
        let health = HealthRegistry::default();
        let before = queue_rows(&conn, &path, "grok-x", &health).unwrap();
        assert_eq!(before[0].learned_format, None, "還沒學到時應為 null");
        learn_format(&path, a, "grok-x", "openai-chat", "openai-responses");
        let after = queue_rows(&conn, &path, "grok-x", &health).unwrap();
        assert_eq!(after[0].learned_format.as_deref(), Some("openai-responses"));
    }

    /// 模型清單：只列啟用來源的模型，並附上「有幾個來源」。
    #[test]
    fn queue_models_counts_sources() {
        let (_d, _path, conn) = db();
        let a = insert_provider(&conn, "A", "openai-chat", 1);
        let b = insert_provider(&conn, "B", "openai-chat", 1);
        let off = insert_provider(&conn, "off", "openai-chat", 0);
        add_model(&conn, a, "both", 1);
        add_model(&conn, b, "both", 1);
        add_model(&conn, a, "only-one", 1);
        add_model(&conn, off, "hidden", 1);
        let list = queue_models(&conn).unwrap();
        assert_eq!(
            list,
            vec![
                QueueModel {
                    model: "both".into(),
                    sources: 2
                },
                QueueModel {
                    model: "only-one".into(),
                    sources: 1
                },
            ],
            "停用來源的模型不該出現；來源多的排前面"
        );
    }
}
