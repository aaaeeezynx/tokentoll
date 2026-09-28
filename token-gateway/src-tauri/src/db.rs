//! SQLite 存儲層（M1）：建表 + WAL + 單連接 Mutex。
//!
//! 資料檔位於系統應用程式數據目錄，Windows 下為
//! `%APPDATA%/com.tokencounter.gateway/app.db`。
//! 表結構已按 M2（上游渠道）/ M3（本地 Key + 請求日誌）/ M4（定價）預留。

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// 當前 schema 版本。後續升級時遞增，並補充 ALTER 遷移。
/// v8（Phase 1）：新增 provider_stripped_fields / proxy_trace（純新增表，
/// 走 SCHEMA 的 CREATE TABLE IF NOT EXISTS，無需 ALTER）。
pub const SCHEMA_VERSION: i32 = 8;

/// 網關預設連接埠（C 方案：可在設定中修改；歷史預設 15721 與 cc-switch 衝突）。
pub const DEFAULT_GATEWAY_PORT: u16 = 15722;

pub struct DbState {
    pub conn: Mutex<Connection>,
    pub path: PathBuf,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);

CREATE TABLE IF NOT EXISTS providers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    app_type TEXT NOT NULL DEFAULT 'claude',
    api_format TEXT NOT NULL DEFAULT 'anthropic',
    base_url TEXT NOT NULL,
    api_key TEXT NOT NULL DEFAULT '',
    auth_scheme TEXT NOT NULL DEFAULT 'bearer',
    models_json TEXT NOT NULL DEFAULT '[]',
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key_prefix TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    key_plain TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL DEFAULT '',
    quota_tokens INTEGER NOT NULL DEFAULT -1,
    used_tokens INTEGER NOT NULL DEFAULT 0,
    rate_limit_qpm INTEGER NOT NULL DEFAULT 60,
    allowed_models_json TEXT NOT NULL DEFAULT '[]',
    allowed_apps_json TEXT NOT NULL DEFAULT '[]',
    provider_id INTEGER,
    expires_at INTEGER,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,
    key_id INTEGER,
    app TEXT NOT NULL DEFAULT '',
    provider_id INTEGER,
    model_raw TEXT NOT NULL DEFAULT '',
    model_norm TEXT NOT NULL DEFAULT '',
    in_tok INTEGER NOT NULL DEFAULT 0,
    out_tok INTEGER NOT NULL DEFAULT 0,
    cache_read INTEGER NOT NULL DEFAULT 0,
    cache_write INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 0,
    is_stream INTEGER NOT NULL DEFAULT 0,
    -- M5 歷史回填：source 'gateway'（預設）| 'import'；import_path 記錄來源
    -- 文件（冪等重掃時按此刪除重插），網關行留空。
    source TEXT NOT NULL DEFAULT 'gateway',
    import_path TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_request_logs_ts ON request_logs(ts);
CREATE INDEX IF NOT EXISTS idx_request_logs_key ON request_logs(key_id);

-- M5：回填冪等游標。文件類 (tool, path) 記 mtime+size；opencode 按會話
-- (tool='opencode', path='opencode:<session_id>') 記 time_updated。
CREATE TABLE IF NOT EXISTS import_state (
    tool TEXT NOT NULL,
    path TEXT NOT NULL,
    fp1 INTEGER NOT NULL DEFAULT 0,
    fp2 INTEGER NOT NULL DEFAULT 0,
    rows_added INTEGER NOT NULL DEFAULT 0,
    imported_at INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tool, path)
);

CREATE TABLE IF NOT EXISTS pricing (
    model_norm TEXT PRIMARY KEY,
    in_pm REAL NOT NULL,
    out_pm REAL NOT NULL,
    cache_read_pm REAL NOT NULL DEFAULT 0,
    cache_create_pm REAL NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);

-- M4.5：渠道定價。mode: usage（按量）| subscription（訂閱）| tou（峰谷）。
-- model_norm = '*' 表示該渠道預設（所有未單獨定價的模型）。
-- subscription：sub_fee_usd（每自然月）+ sub_included_tokens（期內免費額度，-1 無限），
--   超出部分按 in/out 等四率計超額費。
-- tou：時段見 pricing_periods 表；未命中任何時段用基礎四率。
CREATE TABLE IF NOT EXISTS provider_pricing (
    provider_id INTEGER NOT NULL,
    model_norm TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'usage',
    in_pm REAL NOT NULL DEFAULT 0,
    out_pm REAL NOT NULL DEFAULT 0,
    cache_read_pm REAL NOT NULL DEFAULT 0,
    cache_create_pm REAL NOT NULL DEFAULT 0,
    sub_fee_usd REAL NOT NULL DEFAULT 0,
    sub_included_tokens INTEGER NOT NULL DEFAULT -1,
    source TEXT NOT NULL DEFAULT 'custom',
    PRIMARY KEY (provider_id, model_norm)
);

-- M4.10：峰谷多時段（應對 deepseek 類多重峰谷）。ord 越小優先級越高；
-- 同一 (provider_id, model_norm) 時段重疊時取 ord 最小且命中的第一個。
CREATE TABLE IF NOT EXISTS pricing_periods (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id INTEGER NOT NULL,
    model_norm TEXT NOT NULL DEFAULT '*',
    name TEXT NOT NULL DEFAULT '時段',
    start TEXT NOT NULL DEFAULT '09:00',
    end TEXT NOT NULL DEFAULT '18:00',
    ord INTEGER NOT NULL DEFAULT 0,
    in_pm REAL NOT NULL DEFAULT 0,
    out_pm REAL NOT NULL DEFAULT 0,
    cache_read_pm REAL NOT NULL DEFAULT 0,
    cache_create_pm REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_periods_lookup
    ON pricing_periods (provider_id, model_norm, ord);

-- M6：模型目錄抓取緩存（/v1/models 原始 ID 列表）。
CREATE TABLE IF NOT EXISTS model_catalogs (
    provider_id INTEGER PRIMARY KEY,
    fetched_at INTEGER NOT NULL DEFAULT 0,
    models_json TEXT NOT NULL DEFAULT '[]'
);

-- M6：可用模型映射（對標 cc-switch 模型映射）。
-- reasoning: unset | minimal | low | medium | high | xhigh | max | ultra（支援逗號分隔多選）。
-- context_window: NULL = 未設定；作為各工具配置的上限寫入（Codex catalog / OpenCode limit）。
CREATE TABLE IF NOT EXISTS provider_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id INTEGER NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    actual_model TEXT NOT NULL DEFAULT '',
    context_window INTEGER,
    reasoning TEXT NOT NULL DEFAULT 'unset',
    ord INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    UNIQUE(provider_id, display_name)
);
CREATE INDEX IF NOT EXISTS idx_models_lookup
    ON provider_models (provider_id, ord, id);

-- ── Phase 1：可觀測性設施 ────────────────────────────────────────

-- 上游拒收欄位的持久化記憶。
-- 原本只存在 ProxyCtx 的進程記憶體（proxy.rs 的 `stripped`），網關每次
-- 停止／啟動就歸零，導致每個渠道的第一個請求都要重踩一次 400 再重試。
CREATE TABLE IF NOT EXISTS provider_stripped_fields (
    provider_id INTEGER NOT NULL,
    field TEXT NOT NULL,
    learned_at INTEGER NOT NULL,
    PRIMARY KEY (provider_id, field)
);

-- 請求追蹤。刻意「只在異常時寫入」（被剝離欄位、上游 4xx/5xx、body 解析
-- 失敗、重試），正常請求不寫，避免日誌洪水。
-- 隱私：body_sha256 是請求體指紋；body_hex 僅在 body 解析失敗時記錄
-- 前 BODY_HEX_MAX bytes 的十六進位，用於判定「是真解析失敗還是 debug
-- 儀器自己弄壞 body」（見 docs/REFACTORING-PLAN.md §5.2）。
CREATE TABLE IF NOT EXISTS proxy_trace (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    ts              INTEGER NOT NULL,
    trace_level     TEXT NOT NULL DEFAULT 'info',
    app             TEXT NOT NULL DEFAULT '',
    model_raw       TEXT NOT NULL DEFAULT '',
    in_fmt          TEXT NOT NULL DEFAULT '',
    target_fmt      TEXT NOT NULL DEFAULT '',
    trans_kind      TEXT NOT NULL DEFAULT '',
    upstream_status INTEGER NOT NULL DEFAULT 0,
    latency_ms      INTEGER NOT NULL DEFAULT 0,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    stripped_fields TEXT NOT NULL DEFAULT '',
    content_length  INTEGER,
    content_type    TEXT NOT NULL DEFAULT '',
    body_sha256     TEXT NOT NULL DEFAULT '',
    body_hex        TEXT NOT NULL DEFAULT '',
    upstream_error  TEXT NOT NULL DEFAULT '',
    note            TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_proxy_trace_ts ON proxy_trace(ts);
CREATE INDEX IF NOT EXISTS idx_proxy_trace_status ON proxy_trace(upstream_status, ts);
"#;

/// 內置種子定價（美元/百萬 token，source='seed'；未知模型費用記 0，M4 做定價管理）。
/// 只收錄公開資料中穩定的型號，DeepSeek/月之暗面等人民幣計價的上游由用戶在 M4 補。
const SEED_PRICING: &[(&str, f64, f64, f64, f64)] = &[
    ("claude-opus-4-8", 5.0, 25.0, 0.50, 6.25),
    ("claude-opus-4-5", 5.0, 25.0, 0.50, 6.25),
    ("claude-sonnet-4-5", 3.0, 15.0, 0.30, 3.75),
    ("claude-haiku-4-5", 1.0, 5.0, 0.10, 1.25),
    ("gpt-5", 1.25, 10.0, 0.125, 0.0),
    ("gpt-5.1", 1.25, 10.0, 0.125, 0.0),
    ("gpt-5.2", 1.75, 14.0, 0.175, 0.0),
    ("gemini-2.5-pro", 1.25, 10.0, 0.125, 0.0),
    ("gemini-2.5-flash", 0.30, 2.50, 0.03, 0.0),
    // 以下來自實測 + zen 官方文檔（2026-09）：deepseek-v4-flash 經 flatkey 非流式/流式雙驗證；
    // muse-spark-1.3-contributor-free 經 zen /v1/responses 驗證；deepseek-v4-pro 價格取自官方文檔。
    ("deepseek-v4-flash", 0.14, 0.28, 0.028, 0.0),
    ("deepseek-v4-pro", 1.74, 3.48, 0.145, 0.0),
    ("mimo-v2.5-free", 0.0, 0.0, 0.0, 0.0),
    ("muse-spark-1.3-contributor-free", 0.0, 0.0, 0.0, 0.0),
    // zen 官方文檔價（2026-09）：gemini-3.6-flash 經真機驗證可用
    ("gemini-3.6-flash", 1.50, 7.50, 0.15, 0.0),
];

/// 實測可用的預設渠道（2026-09 真機驗證；api_key 留空，用戶在 UI 填自己的 key）。
/// zen 用公共前綴 base，模型決定走 chat 還是 responses（見官方文檔端點表）。
const SEED_PROVIDERS: &[(&str, &str, &str, &str, &str, &str)] = &[
    // (name, app_type, api_format, base_url, auth_scheme, models_json)
    (
        "flatkey",
        "universal",
        "openai-chat",
        "https://router.flatkey.ai/v1",
        "bearer",
        r#"["deepseek-v4-flash"]"#,
    ),
    (
        "zen",
        "universal",
        "mixed",
        "https://opencode.ai/zen/v1",
        "bearer",
        r#"["muse-spark-1.3-contributor-free", "mimo-v2.5-free"]"#,
    ),
    (
        "nvidia",
        "universal",
        "openai-chat",
        "https://integrate.api.nvidia.com/v1",
        "bearer",
        r#"["deepseek-ai/deepseek-v4-pro-0813", "nvidia/nemotron-3-ultra-550b-a55b"]"#,
    ),
    (
        "gemini",
        "universal",
        "gemini",
        "https://generativelanguage.googleapis.com",
        "goog-key",
        r#"["gemini-3.6-flash"]"#,
    ),
    (
        "anthropic",
        "universal",
        "anthropic",
        "https://api.anthropic.com",
        "anthropic",
        r#"[]"#,
    ),
];

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

pub(crate) fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .ok()
}

pub(crate) fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

pub fn init_db(app: &AppHandle) -> Result<DbState, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("app.db");
    let conn = open_and_ensure(&path)?;
    Ok(DbState {
        conn: Mutex::new(conn),
        path,
    })
}

// ─────────────────────────────────────────────────────── 單測 ───
//
// 本檔是資料真相層：schema 建錯或 migration 漏跑等於使用者資料損毀。
// 在 Phase 1 之前這裡是 0 測試（見 docs/REFACTORING-PLAN.md §3 B8）。
#[cfg(test)]
mod tests {
    use super::*;

    fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
        conn.prepare(&format!(
            "SELECT 1 FROM pragma_table_info('{table}') WHERE name='{col}'"
        ))
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false)
    }

    fn has_table(conn: &Connection, table: &str) -> bool {
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
            .and_then(|mut s| s.exists([table]))
            .unwrap_or(false)
    }

    fn max_version(conn: &Connection) -> i64 {
        conn.query_row("SELECT COALESCE(MAX(version),0) FROM schema_version", [], |r| {
            r.get(0)
        })
        .unwrap_or(0)
    }

    fn open(path: &Path) -> Connection {
        open_and_ensure(path).expect("open_and_ensure")
    }

    #[test]
    fn fresh_db_reaches_current_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64);
    }

    #[test]
    fn fresh_db_has_all_core_tables() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        for t in [
            "schema_version",
            "providers",
            "local_keys",
            "request_logs",
            "import_state",
            "pricing",
            "settings",
            "provider_pricing",
            "pricing_periods",
            "provider_models",
            "provider_stripped_fields",
            "proxy_trace",
        ] {
            assert!(has_table(&c, t), "缺少表 {t}");
        }
    }

    fn count_rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(-1)
    }

    /// v7 → v8 升級：**Phase 1 之前建立的資料庫升級後必須原樣保留既有資料。**
    ///
    /// 這條路徑值得單獨釘住，因為它是**真實使用者第一次啟動新版本時唯一會走
    /// 的路**：`%APPDATA%\com.tokencounter.gateway\app.db` 是長期累積的檔案
    /// （實際測到的現況：v7、7 個 providers、4,111 筆 request_logs）。升級若把
    /// 資料弄丟或讓程式起不來，代價是使用者的全部歷史。
    ///
    /// 作法：造一個「除 Phase 1 那兩張表以外都是最新形狀」的資料庫，再把版本
    /// 退回 7。因為 Phase 1 只**新增**表與索引、沒有動任何既有表的欄位，這與
    /// 真實的 v7 資料庫等價。
    #[test]
    fn v7_db_without_phase1_tables_upgrades_preserving_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            c.execute(
                "INSERT INTO providers (name, app_type, api_format, base_url, api_key,
                 models_json, priority, enabled, created_at, updated_at)
                 VALUES ('p','codex','openai-chat','http://x','k','[]',0,1,0,0)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok,
                 cost_usd, latency_ms, status, is_stream)
                 VALUES (1,'codex','m','m',1,2,0.0,10,200,0)",
                [],
            )
            .unwrap();
            // 退回 v7：拿掉 Phase 1 才有的兩張表與對應版本列
            c.execute_batch(
                "DROP TABLE proxy_trace;
                 DROP TABLE provider_stripped_fields;
                 DELETE FROM schema_version WHERE version >= 8;",
            )
            .unwrap();
            assert!(!has_table(&c, "proxy_trace"), "前置條件：v7 沒有 proxy_trace");
            assert_eq!(max_version(&c), 7, "前置條件：版本應為 7");
        }

        // 比較「升級前 vs 升級後」，而不是比對寫死的數字：`open_and_ensure`
        // 在全新資料庫上會**播種** providers／pricing（見
        // `fresh_db_seeds_providers_and_pricing_once`），所以第一次 `open`
        // 之後 providers 已經不只是上面插入的那一筆。
        let before = {
            let c = Connection::open(&path).unwrap();
            (count_rows(&c, "providers"), count_rows(&c, "request_logs"))
        };
        assert!(before.0 > 0 && before.1 > 0, "前置條件：應有既有資料");

        // 再次開啟 == 使用者啟動新版
        let c = open(&path);
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64, "版本應升到 8");
        assert!(has_table(&c, "proxy_trace"), "升級後應補回 proxy_trace");
        assert!(
            has_table(&c, "provider_stripped_fields"),
            "升級後應補回 provider_stripped_fields"
        );
        assert_eq!(
            count_rows(&c, "providers"),
            before.0,
            "既有 providers 不得遺失"
        );
        assert_eq!(
            count_rows(&c, "request_logs"),
            before.1,
            "既有 request_logs 不得遺失"
        );
    }

    /// 對**指定的真實資料庫副本**跑一次升級，用來在出貨前確認使用者的檔案能
    /// 安全升級、資料不減。
    ///
    /// 先複製再跑，切勿直接指向正式檔：
    /// ```text
    /// copy "%APPDATA%\com.tokencounter.gateway\app.db" "%TEMP%\app_copy.db"
    /// set TOKEN_GATEWAY_MIGRATE_DB=%TEMP%\app_copy.db
    /// cargo test live_migrate_real_db_copy -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn live_migrate_real_db_copy() {
        let Ok(path) = std::env::var("TOKEN_GATEWAY_MIGRATE_DB") else {
            eprintln!("略過：未設定 TOKEN_GATEWAY_MIGRATE_DB");
            return;
        };
        let p = std::path::PathBuf::from(&path);
        let before = {
            let c = Connection::open(&p).expect("開啟副本");
            (
                max_version(&c),
                count_rows(&c, "providers"),
                count_rows(&c, "request_logs"),
            )
        };
        println!(
            "升級前：version={} providers={} request_logs={}",
            before.0, before.1, before.2
        );
        let c = open_and_ensure(&p).expect("升級失敗");
        let after = (
            max_version(&c),
            count_rows(&c, "providers"),
            count_rows(&c, "request_logs"),
            has_table(&c, "proxy_trace"),
            has_table(&c, "provider_stripped_fields"),
        );
        println!(
            "升級後：version={} providers={} request_logs={} proxy_trace={} provider_stripped_fields={}",
            after.0, after.1, after.2, after.3, after.4
        );
        assert_eq!(after.0, SCHEMA_VERSION as i64, "版本應升到最新");
        assert_eq!(after.1, before.1, "providers 不得增減");
        assert_eq!(after.2, before.2, "request_logs 不得增減");
        assert!(after.3 && after.4, "Phase 1 的兩張表應被補上");
    }

    /// **自動化版的 migration 測試（取代只能人工跑的 `live_migrate_real_db_copy`）。**
    ///
    /// 對「每一個歷史版本」各造一個資料庫，逐一確認升級到最新版之後：
    /// 1. 版本升到 [`SCHEMA_VERSION`]
    /// 2. 既有資料一列不少（providers／request_logs／settings）
    /// 3. 最新結構該有的表全部存在
    ///
    /// 為什麼要逐版跑而不是只跑 v7：`open_and_ensure` 是「一次補到最新」，
    /// 真實使用者可能停在**任何**一個舊版，任何一版的落差都會讓他開不起來。
    /// 這個測試讓「每個起點」都被走過一次，且不需人工介入。
    ///
    /// 作法：先建到最新，記下所有表名，再把「比目標版本新的表」與「較新的
    /// schema_version 列」刪掉，即得該版本的等價資料庫（與
    /// `v7_db_without_phase1_tables_upgrades_preserving_data` 同一個手法，
    /// 但對所有版本自動化）。
    #[test]
    fn every_legacy_version_upgrades_preserving_data() {
        // 各版本「新增」的表；升級到該版時應該要有。
        // v8 = Phase 1 的兩張觀測表；其餘版本沒有新增表（改欄位／索引）。
        for target in 1..SCHEMA_VERSION as i64 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("v{target}.db"));
            {
                let c = open(&path);
                // 放進可辨識的既有資料
                c.execute(
                    "INSERT INTO providers (name, app_type, api_format, base_url, api_key,
                     models_json, priority, enabled, created_at, updated_at)
                     VALUES ('keepme','codex','openai-chat','http://x','k','[]',0,1,0,0)",
                    [],
                )
                .unwrap();
                for i in 0..5 {
                    c.execute(
                        "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok,
                         cost_usd, latency_ms, status, is_stream)
                         VALUES (?1,'codex','m','m',1,2,0.0,10,200,0)",
                        [1000 + i],
                    )
                    .unwrap();
                }
                c.execute(
                    "INSERT INTO settings (key, value) VALUES ('probe','v')",
                    [],
                )
                .unwrap();

                // 退回目標版本：刪掉比它新的表與版本列
                if target < 8 {
                    c.execute_batch(
                        "DROP TABLE IF EXISTS proxy_trace;
                         DROP TABLE IF EXISTS provider_stripped_fields;",
                    )
                    .unwrap();
                }
                c.execute(
                    "DELETE FROM schema_version WHERE version > ?1",
                    [target],
                )
                .unwrap();
                assert_eq!(max_version(&c), target, "前置條件：版本應為 {target}");
            }

            let before = {
                let c = Connection::open(&path).unwrap();
                (
                    count_rows(&c, "providers"),
                    count_rows(&c, "request_logs"),
                    count_rows(&c, "settings"),
                )
            };

            // 再次開啟 == 使用者啟動新版
            let c = open(&path);
            assert_eq!(
                max_version(&c),
                SCHEMA_VERSION as i64,
                "v{target} 應升級到 {}",
                SCHEMA_VERSION
            );
            assert_eq!(
                count_rows(&c, "providers"),
                before.0,
                "v{target} 升級後 providers 不得增減"
            );
            assert_eq!(
                count_rows(&c, "request_logs"),
                before.1,
                "v{target} 升級後 request_logs 不得增減"
            );
            // settings 可能被升級流程「補上」新鍵（種子），所以只驗既有鍵沒被動。
            assert!(
                count_rows(&c, "settings") >= before.2,
                "v{target} 升級後 settings 不得減少"
            );
            assert_eq!(
                c.query_row(
                    "SELECT value FROM settings WHERE key='probe'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "v",
                "v{target} 升級後設定值不得變"
            );
            // 最新結構的表必須齊全（與 fresh_db_has_all_core_tables 同一份清單）
            for t in [
                "schema_version",
                "providers",
                "local_keys",
                "request_logs",
                "import_state",
                "pricing",
                "settings",
                "provider_pricing",
                "pricing_periods",
                "provider_models",
                "provider_stripped_fields",
                "proxy_trace",
            ] {
                assert!(has_table(&c, t), "v{target} 升級後缺少表 {t}");
            }
        }
    }

    #[test]
    fn open_and_ensure_is_idempotent() {        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let c1 = open(&path);
        assert_eq!(max_version(&c1), SCHEMA_VERSION as i64);
        drop(c1);
        // 再開一次不得報錯、版本不得重複寫入
        let c2 = open(&path);
        assert_eq!(max_version(&c2), SCHEMA_VERSION as i64);
        let rows: i64 = c2
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, SCHEMA_VERSION as i64, "版本列應逐版一列，不重複");
    }

    #[test]
    fn phase1_tables_are_recreated_on_legacy_db() {
        // 模擬 Phase 1 之前的資料庫：兩張新表不存在。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = open(&path);
            c.execute_batch(
                "DROP TABLE proxy_trace; DROP TABLE provider_stripped_fields;",
            )
            .unwrap();
            assert!(!has_table(&c, "proxy_trace"));
        }
        // 重新開啟應自動補回（SCHEMA 的 CREATE TABLE IF NOT EXISTS）
        let c = open(&path);
        assert!(has_table(&c, "proxy_trace"), "proxy_trace 未補回");
        assert!(
            has_table(&c, "provider_stripped_fields"),
            "provider_stripped_fields 未補回"
        );
        // 索引也要在
        let idx: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
                 AND name IN ('idx_proxy_trace_ts','idx_proxy_trace_status')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 2, "proxy_trace 索引未建立");
    }

    #[test]
    fn legacy_local_keys_gains_provider_id_and_key_plain() {
        // v1 → v2（provider_id）與 v6 → v7（key_plain）的 ALTER 路徑。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE local_keys (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    key_prefix TEXT NOT NULL,
                    key_hash TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL DEFAULT '',
                    quota_tokens INTEGER NOT NULL DEFAULT -1,
                    used_tokens INTEGER NOT NULL DEFAULT 0,
                    rate_limit_qpm INTEGER NOT NULL DEFAULT 60,
                    allowed_models_json TEXT NOT NULL DEFAULT '[]',
                    allowed_apps_json TEXT NOT NULL DEFAULT '[]',
                    expires_at INTEGER,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL
                 );
                 INSERT INTO local_keys (key_prefix, key_hash, created_at)
                 VALUES ('sk-local-abc', 'hash1', 1000);",
            )
            .unwrap();
        }
        let c = open(&path);
        assert!(has_column(&c, "local_keys", "provider_id"));
        assert!(has_column(&c, "local_keys", "key_plain"));
        // 既有資料必須保留，新欄位取預設值
        let (prefix, pid, plain, created): (String, Option<i64>, String, i64) = c
            .query_row(
                "SELECT key_prefix, provider_id, key_plain, created_at FROM local_keys",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(prefix, "sk-local-abc");
        assert_eq!(pid, None);
        assert_eq!(plain, "");
        assert_eq!(created, 1000);
    }

    #[test]
    fn legacy_request_logs_gains_source_and_import_path() {
        // v5 → v6（M5 歷史回填）的 ALTER 路徑。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE request_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    ts INTEGER NOT NULL,
                    key_id INTEGER,
                    app TEXT NOT NULL DEFAULT '',
                    provider_id INTEGER,
                    model_raw TEXT NOT NULL DEFAULT '',
                    model_norm TEXT NOT NULL DEFAULT '',
                    in_tok INTEGER NOT NULL DEFAULT 0,
                    out_tok INTEGER NOT NULL DEFAULT 0,
                    cache_read INTEGER NOT NULL DEFAULT 0,
                    cache_write INTEGER NOT NULL DEFAULT 0,
                    cost_usd REAL NOT NULL DEFAULT 0,
                    latency_ms INTEGER NOT NULL DEFAULT 0,
                    status INTEGER NOT NULL DEFAULT 0,
                    is_stream INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT INTO request_logs (ts, app) VALUES (500, 'codex');",
            )
            .unwrap();
        }
        let c = open(&path);
        assert!(has_column(&c, "request_logs", "source"));
        assert!(has_column(&c, "request_logs", "import_path"));
        let (app, source, ip): (String, String, String) = c
            .query_row(
                "SELECT app, source, import_path FROM request_logs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(app, "codex");
        assert_eq!(source, "gateway", "舊列應補上預設來源");
        assert_eq!(ip, "");
        // ALTER 之後才建的索引必須存在
        let idx: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
                 AND name IN ('idx_logs_import','idx_logs_dedupe')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 2, "v6 索引未建立");
    }

    #[test]
    fn legacy_tou_pricing_migrates_to_periods_and_drops_peak_columns() {
        // v3 → v4：單窗口峰谷 → pricing_periods 多時段，然後 DROP 舊列。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE provider_pricing (
                    provider_id INTEGER NOT NULL,
                    model_norm TEXT NOT NULL,
                    mode TEXT NOT NULL DEFAULT 'usage',
                    in_pm REAL NOT NULL DEFAULT 0,
                    out_pm REAL NOT NULL DEFAULT 0,
                    cache_read_pm REAL NOT NULL DEFAULT 0,
                    cache_create_pm REAL NOT NULL DEFAULT 0,
                    peak_start TEXT NOT NULL DEFAULT '',
                    peak_end TEXT NOT NULL DEFAULT '',
                    peak_in_pm REAL,
                    peak_out_pm REAL,
                    peak_cache_read_pm REAL,
                    peak_cache_create_pm REAL
                 );
                 INSERT INTO provider_pricing
                   (provider_id, model_norm, mode, in_pm, out_pm, peak_start, peak_end,
                    peak_in_pm, peak_out_pm, peak_cache_read_pm, peak_cache_create_pm)
                 VALUES (3, '*', 'tou', 1.0, 2.0, '08:00', '20:00', 5.0, 6.0, 0.5, 0.6);
                 -- 非 tou 列不應被遷移
                 INSERT INTO provider_pricing (provider_id, model_norm, mode, in_pm, out_pm, peak_start, peak_end)
                 VALUES (4, 'm', 'usage', 1.0, 2.0, '', '');",
            )
            .unwrap();
        }
        let c = open(&path);
        // 舊欄位已移除
        for col in ["peak_start", "peak_end", "peak_in_pm", "peak_out_pm"] {
            assert!(!has_column(&c, "provider_pricing", col), "{col} 未移除");
        }
        // 峰時段已遷移
        let (name, start, end, ipm, opm, cr, cc): (String, String, String, f64, f64, f64, f64) = c
            .query_row(
                "SELECT name, start, end, in_pm, out_pm, cache_read_pm, cache_create_pm
                 FROM pricing_periods WHERE provider_id=3 AND model_norm='*'",
                [],
                |r| {
                    Ok((
                        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(name, "峰時");
        assert_eq!((start.as_str(), end.as_str()), ("08:00", "20:00"));
        assert_eq!((ipm, opm, cr, cc), (5.0, 6.0, 0.5, 0.6));
        // 非 tou 列不產生時段
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM pricing_periods WHERE provider_id=4",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        // 原始列保留（遷移只補時段，不刪定價列）
        let kept: i64 = c
            .query_row("SELECT COUNT(*) FROM provider_pricing", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 2);
    }

    #[test]
    fn settings_defaults_seeded_without_overwriting_user_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            assert_eq!(
                get_setting(&c, "gateway_port").as_deref(),
                Some(DEFAULT_GATEWAY_PORT.to_string().as_str())
            );
            assert_eq!(get_setting(&c, "accent").as_deref(), Some("blue"));
            // 使用者改過之後重開不得被覆蓋
            set_setting(&c, "accent", "purple").unwrap();
            set_setting(&c, "gateway_port", "19999").unwrap();
        }
        let c = open(&path);
        assert_eq!(get_setting(&c, "accent").as_deref(), Some("purple"));
        assert_eq!(get_setting(&c, "gateway_port").as_deref(), Some("19999"));
    }

    #[test]
    fn seeds_are_not_revived_after_user_deletes_them() {
        // 種子只在 seed_version 落後時下發；使用者刪除後重開不應復活。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            c.execute("DELETE FROM pricing", []).unwrap();
            c.execute("DELETE FROM providers", []).unwrap();
        }
        let c = open(&path);
        let p: i64 = c
            .query_row("SELECT COUNT(*) FROM pricing", [], |r| r.get(0))
            .unwrap();
        let v: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, 0, "使用者刪除的種子定價不應復活");
        assert_eq!(v, 0, "使用者刪除的種子渠道不應復活");
    }

    #[test]
    fn fresh_db_seeds_providers_and_pricing_once() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        let p: i64 = c
            .query_row("SELECT COUNT(*) FROM pricing WHERE source='seed'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(p as usize, SEED_PRICING.len());
        let v: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, SEED_PROVIDERS.len());
        // 種子渠道的 key 必須是空的（絕不內建金鑰）
        let nonempty: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM providers WHERE api_key <> ''",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nonempty, 0, "種子渠道不得內建任何金鑰");
    }
}
