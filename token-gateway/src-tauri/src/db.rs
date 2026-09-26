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
pub const SCHEMA_VERSION: i32 = 7;

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
