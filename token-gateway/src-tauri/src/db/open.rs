//! 開啟資料庫並保證 schema（新建 + 冪等遷移 + 種子下發）。
//!
//! 由 `db.rs` 拆分而來；內容與行為一字未改。

use rusqlite::Connection;
use std::path::Path;

use super::settings::{get_setting, set_setting};
use super::{
    DEFAULT_GATEWAY_PORT, SCHEMA, SCHEMA_VERSION, SEED_PRICING, SEED_PROVIDERS,
};

/// 打開資料庫並保證 schema（新建 + 冪等遷移），供 init/代理任務/單測共用。
pub(crate) fn open_and_ensure(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    // v1 → v2：local_keys.provider_id（新庫建表已含，老庫補列）
    let has_col: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('local_keys') WHERE name='provider_id'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if !has_col {
        conn.execute("ALTER TABLE local_keys ADD COLUMN provider_id INTEGER", [])
            .map_err(|e| e.to_string())?;
    }
    // M4.11 列名 api_key_enc → api_key（老庫改名，新庫建表已是新名）
    let has_old: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('providers') WHERE name='api_key_enc'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    let has_new: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('providers') WHERE name='api_key'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if has_old && !has_new {
        conn.execute("ALTER TABLE providers RENAME COLUMN api_key_enc TO api_key", [])
            .map_err(|e| e.to_string())?;
    }
    // Gemini 原生：在 providers 補 auth_scheme（bearer | goog-key），新庫建表已含
    let has_scheme: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('providers') WHERE name='auth_scheme'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if !has_scheme {
        conn.execute(
            "ALTER TABLE providers ADD COLUMN auth_scheme TEXT NOT NULL DEFAULT 'bearer'",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    // v3 → v4：單窗口峰谷遷移到 pricing_periods 多時段，然後刪除舊列
    let has_peak: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('provider_pricing') WHERE name='peak_start'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if has_peak {
        conn.execute(
            "INSERT INTO pricing_periods (provider_id, model_norm, name, start, end, ord,
             in_pm, out_pm, cache_read_pm, cache_create_pm)
             SELECT provider_id, model_norm, '峰時', peak_start, peak_end, 0,
                    COALESCE(peak_in_pm, in_pm), COALESCE(peak_out_pm, out_pm),
                    COALESCE(peak_cache_read_pm, cache_read_pm),
                    COALESCE(peak_cache_create_pm, cache_create_pm)
             FROM provider_pricing
             WHERE mode = 'tou'
             AND NOT EXISTS (SELECT 1 FROM pricing_periods p
                             WHERE p.provider_id = provider_pricing.provider_id
                             AND p.model_norm = provider_pricing.model_norm)",
            [],
        )
        .map_err(|e| e.to_string())?;
        for col in [
            "peak_start",
            "peak_end",
            "peak_in_pm",
            "peak_out_pm",
            "peak_cache_read_pm",
            "peak_cache_create_pm",
        ] {
            conn.execute(
                &format!("ALTER TABLE provider_pricing DROP COLUMN {col}"),
                [],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // 種子數據（定價+預設渠道）：只在 seed_version 落後時跑一次。
    // 用戶刪除後不會復活；新增種子時遞增下方版本號即可再次下發（僅補缺失）。
    const SEED_VERSION: i64 = 3;
    let seed_ver: i64 = get_setting(&conn, "seed_version")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if seed_ver < SEED_VERSION {
        // 種子定價（INSERT OR IGNORE，冪等）
        for (m, i, o, cr, cw) in SEED_PRICING {
            conn.execute(
                "INSERT OR IGNORE INTO pricing (model_norm, in_pm, out_pm, cache_read_pm, cache_create_pm, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'seed')",
                rusqlite::params![m, i, o, cr, cw],
            )
            .map_err(|e| e.to_string())?;
        }
        // 預設渠道（按名稱冪等，用戶改過的不覆蓋；key 恆為空，由用戶在 UI 填寫）
        for (name, app_type, api_format, base_url, auth_scheme, models_json) in SEED_PROVIDERS {
            conn.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, auth_scheme, api_key, models_json, priority, enabled, created_at, updated_at)
                 SELECT ?1, ?2, ?3, ?4, ?5, '', ?6, 0, 1, ?7, ?7
                 WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = ?1)",
                rusqlite::params![
                    name,
                    app_type,
                    api_format,
                    base_url,
                    auth_scheme,
                    models_json,
                    crate::fsutil::now_ms()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        set_setting(&conn, "seed_version", &SEED_VERSION.to_string())
            .map_err(|e| e.to_string())?;
    }
    // 預設設定（不覆蓋用戶已改的值）
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('gateway_port', ?1)",
        [DEFAULT_GATEWAY_PORT.to_string()],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_start_proxy', '0')",
        [],
    )
    .map_err(|e| e.to_string())?;
    // 外觀強調色（持久真相源；前端 localStorage 只做首幀快取）
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('accent', 'blue')",
        [],
    )
    .map_err(|e| e.to_string())?;
    // v4 → v5：providers.models_json 導入 provider_models（每渠道僅在尚無行時導入一次）
    {
        let mut stmt = conn
            .prepare("SELECT id, models_json FROM providers")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        for (pid, mj) in rows {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM provider_models WHERE provider_id = ?1",
                    [pid],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if count > 0 {
                continue;
            }
            let models: Vec<String> = serde_json::from_str(&mj).unwrap_or_default();
            for (i, m) in models.iter().enumerate() {
                let m = m.trim();
                if m.is_empty() {
                    continue;
                }
                conn.execute(
                    "INSERT OR IGNORE INTO provider_models
                     (provider_id, display_name, actual_model, reasoning, ord, enabled)
                     VALUES (?1, ?2, ?2, 'unset', ?3, 1)",
                    rusqlite::params![pid, m, (i as i64) * 10],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    // v5 → v6（M5 歷史回填）：request_logs 補 source / import_path，新庫建表已含
    for col in ["source", "import_path"] {
        let has: bool = conn
            .prepare(&format!(
                "SELECT 1 FROM pragma_table_info('request_logs') WHERE name='{col}'"
            ))
            .map_err(|e| e.to_string())?
            .exists([])
            .map_err(|e| e.to_string())?;
        if !has {
            let default = if col == "source" { "'gateway'" } else { "''" };
            conn.execute(
                &format!(
                    "ALTER TABLE request_logs ADD COLUMN {col} TEXT NOT NULL DEFAULT {default}"
                ),
                [],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // v6 → v7：local_keys 明文留存以便簽發後再次查看（舊庫補列，舊 Key 留空）
    let has_plain: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('local_keys') WHERE name='key_plain'")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?;
    if !has_plain {
        conn.execute(
            "ALTER TABLE local_keys ADD COLUMN key_plain TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    // v13 → v14（2026-10-02）：MCP／提示詞／技能三個功能已移除，連帶把只服務它們的
    // 六張表清掉。SCHEMA 已不再建立這些表，所以對新舊庫都安全：舊庫把殘留的表 DROP，
    // 新庫本來就沒有（`IF EXISTS`）。
    //
    // **只動這個 App 自己的庫**：使用者已經寫進各工具設定檔的內容（`AGENTS.md`、
    // `config.toml` 的 `[mcp_servers]`、各工具 `skills/` 目錄與母本）完全不碰 ——
    // 移除的是本 App 的管理能力，不是使用者的檔案。
    for t in [
        "mcp_servers",
        "mcp_bindings",
        "prompt_presets",
        "skill_repos",
        "skills",
        "skill_bindings",
    ] {
        conn.execute(&format!("DROP TABLE IF EXISTS {t}"), [])
            .map_err(|e| e.to_string())?;
    }
    // v14 → v15（2026-10-02）：Deep Link 匯入／資料庫備份／更新檢查／雲端同步
    // 四個功能已移除。與 v14 不同 —— 這四個都沒有專屬資料表，所以不需要 DROP；
    // 但它們的狀態放在通用的 `settings` 表裡，不清就會變成沒人讀的孤兒列。
    purge_removed_feature_settings(&conn).map_err(|e| e.to_string())?;
    // 列齊了再建索引（整批 SCHEMA 先於 ALTER，老庫在此之前無此列）
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_logs_import ON request_logs(import_path)",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_logs_dedupe
         ON request_logs(source, app, model_norm, in_tok, out_tok)",
        [],
    )
    .map_err(|e| e.to_string())?;
    // 清掉指向已不存在來源的孤兒列（見 purge_orphans）。
    let purged = purge_orphans(&conn).map_err(|e| e.to_string())?;
    if purged > 0 {
        eprintln!("gateway: 清掉 {purged} 列孤兒資料（來源已不存在）");
    }
    // 版本號（逐版補寫，多行共存表示歷經版本）
    let ver: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    for v in (ver + 1)..=(SCHEMA_VERSION as i64) {
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (?1)",
            [v],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(conn)
}

/// 清掉「指向已不存在來源」的孤兒列，回傳清掉的總列數。
///
/// 由來（2026-10-01）：實機驗證來源匯出時發現使用者的 `provider_models` 有 12 列
/// 指向 `providers` 裡已經不存在的 id（1/2/3/4/7/10/12）—— 那是早期刪除來源時
/// 還沒有級聯清理留下的殘骸。它們永遠讀不到（`providers.id` 是 AUTOINCREMENT，
/// id 不會被重用），但會讓「這個庫裡到底有幾個模型」這類計數對不上，
/// 匯出的檔案看起來也像漏了東西。
///
/// **刻意不動 `local_keys`**：金鑰綁到已刪除的來源是保留的設計 ——
/// 請求會回明確的 500「綁定的上游渠道不存在」，比靜默失效好查。
/// 所以這裡只清「來源的子表」。
fn purge_orphans(conn: &Connection) -> rusqlite::Result<usize> {
    let mut total = 0;
    for table in [
        "provider_models",
        "provider_pricing",
        "pricing_periods",
        "provider_stripped_fields",
        "provider_model_protocol",
        "provider_usage_query",
        "model_catalogs",
    ] {
        total += conn.execute(
            &format!("DELETE FROM {table} WHERE provider_id NOT IN (SELECT id FROM providers)"),
            [],
        )?;
    }
    Ok(total)
}

/// 清掉只服務「已移除的四個功能」的 `settings` 列，回傳清掉的列數。
///
/// 來由（2026-10-02）：Deep Link 匯入／資料庫備份／更新檢查／雲端同步移除。
/// 這四個功能**沒有專屬資料表**，全部狀態都塞在通用的 `settings` key-value 表，
/// 所以沒有東西可以 DROP —— 留下的就是沒人再讀的孤兒列。
///
/// 真正非清不可的理由是 `cloud_password`：**明文的 WebDAV 密碼**。功能拿掉了
/// 但憑證還躺在 app.db 裡，是實質的資訊留存，不只是垃圾資料。
///
/// 冪等：老庫有就清、沒有就什麼都不做（`key IN (...)` 命中 0 列不報錯），
/// 對新庫也安全。**只點名這 17 個 key** —— `settings` 裡還有 gateway_port、
/// theme、lang、accent、close_action 等在用的設定，用 `DELETE FROM settings`
/// 會把它們一起刪掉。
fn purge_removed_feature_settings(conn: &Connection) -> rusqlite::Result<usize> {
    let keys = [
        // 雲端同步（P4.10）
        "cloud_provider",
        "cloud_base_url",
        "cloud_username",
        "cloud_password",
        "cloud_remote_root",
        "cloud_profile",
        "cloud_auto_sync",
        "cloud_last_sync_ms",
        "cloud_last_result",
        "cloud_last_ok",
        "cloud_last_mtime",
        "cloud_last_bytes",
        // 資料庫備份（P4.8）
        "db_backup_interval",
        "db_backup_keep",
        // 更新檢查（P4.9）
        "update_manifest_url",
        "update_last_check_ms",
        "update_last_result",
    ];
    let list = keys
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(",");
    let purged = conn.execute(&format!("DELETE FROM settings WHERE key IN ({list})"), [])?;
    if purged > 0 {
        eprintln!("gateway: 清掉 {purged} 列已移除功能的設定（雲端同步／備份／更新）");
    }
    Ok(purged)
}
