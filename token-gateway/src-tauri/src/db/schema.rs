//! 建表 SQL（`SCHEMA`）與內置種子資料（定價／預設渠道）。
//!
//! 由 `db.rs` 拆分而來；內容與行為一字未改。

pub(crate) const SCHEMA: &str = r#"
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

-- 學到的「上游協議」記憶（2026-09-30）。
-- 同一個來源的模型可能**逐模型**只在一種端點上架：對 opencode-go 實測，
-- grok-4.7 只在 /responses、mimo-v2.6 只在 /chat/completions、deepseek-* 兩邊都有。
-- 網關先用宣告協議打；被上游回「此模型不支援本協議」就換另一種協議重送，
-- 成功後把結果記在這裡，之後第一個就試它。
-- `declared_format` 是鍵的一部分：使用者把來源協議改掉是**明確的設定變更**，
-- 不可以被舊的學習結果蓋掉（否則會出現「改了設定卻沒生效」）。
CREATE TABLE IF NOT EXISTS provider_model_protocol (
    provider_id     INTEGER NOT NULL,
    model           TEXT NOT NULL,
    declared_format TEXT NOT NULL,
    actual_format   TEXT NOT NULL,
    learned_at      INTEGER NOT NULL,
    PRIMARY KEY (provider_id, model, declared_format)
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

-- 用量查詢設定（P2.1，對標 cc-switch 的 Usage Query）。
-- 每個來源一列：要不要查、用哪個樣板、查哪個網址、怎麼從回應裡取出數字。
-- 金鑰與 token 只留在本機（跟 providers.api_key 一樣是明碼，M3 加固項）。
CREATE TABLE IF NOT EXISTS provider_usage_query (
    provider_id       INTEGER PRIMARY KEY,
    enabled           INTEGER NOT NULL DEFAULT 0,
    template          TEXT NOT NULL DEFAULT 'custom',
    base_url          TEXT NOT NULL DEFAULT '',
    api_key           TEXT NOT NULL DEFAULT '',
    access_token      TEXT NOT NULL DEFAULT '',
    user_id           TEXT NOT NULL DEFAULT '',
    url_template      TEXT NOT NULL DEFAULT '',
    method            TEXT NOT NULL DEFAULT 'GET',
    headers_json      TEXT NOT NULL DEFAULT '[]',
    extractor_json    TEXT NOT NULL DEFAULT '{}',
    timeout_secs      INTEGER NOT NULL DEFAULT 10,
    auto_interval_min INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL DEFAULT 0
);

-- MCP 伺服器（P3.1，對標 cc-switch 的 MCP Management）。
-- 這裡只存「我們管的」伺服器：清單本身與每個工具要不要啟用。
-- 使用者自己在工具設定檔裡手寫、沒匯入的伺服器一律不碰（與 cc-switch 同規則）。
CREATE TABLE IF NOT EXISTS mcp_servers (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT NOT NULL UNIQUE,          -- Server ID（唯一識別）
    name        TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    transport   TEXT NOT NULL DEFAULT 'stdio', -- stdio | http | sse
    command     TEXT NOT NULL DEFAULT '',
    args_json   TEXT NOT NULL DEFAULT '[]',
    url         TEXT NOT NULL DEFAULT '',
    headers_json TEXT NOT NULL DEFAULT '[]',
    env_json    TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL DEFAULT 0
);

-- 每個伺服器對每個工具的啟用狀態（cc-switch 的 app binding）。
CREATE TABLE IF NOT EXISTS mcp_bindings (
    server_id INTEGER NOT NULL,
    app       TEXT NOT NULL,                   -- claude | codex | opencode
    enabled   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (server_id, app)
);
"#;

/// 內置種子定價（美元/百萬 token，source='seed'；未知模型費用記 0，M4 做定價管理）。
/// 只收錄公開資料中穩定的型號，DeepSeek/月之暗面等人民幣計價的上游由用戶在 M4 補。
pub(crate) const SEED_PRICING: &[(&str, f64, f64, f64, f64)] = &[
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
pub(crate) const SEED_PROVIDERS: &[(&str, &str, &str, &str, &str, &str)] = &[
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
