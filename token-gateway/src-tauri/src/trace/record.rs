//! 追蹤層級與追蹤紀錄本體。

use super::{hex_prefix, sha256_hex_bytes, truncate_chars, BODY_HEX_MAX, UPSTREAM_ERR_MAX};

/// 追蹤層級。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceLevel {
    /// 有剝離欄位或有重試，但最終成功。
    #[default]
    Info,
    /// 上游 4xx/5xx、連線失敗、body 解析失敗，**以及網關自己拒絕的請求**
    /// （見 `proxy::logging::reject`：401／403／413／429／500 與模型不在
    /// 清單內的 400）。這一類沒有上游，`upstream_status` 記的是
    /// **回給客戶端的狀態碼**。
    Warn,
}

impl TraceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceLevel::Info => "info",
            TraceLevel::Warn => "warn",
        }
    }
}

/// 一筆請求追蹤。
///
/// 用結構體 + [`Default`] 而非位置參數，因為呼叫端是 800 行的
/// `proxy_handler`，各分支只關心其中幾個欄位。
#[derive(Debug, Clone, Default)]
pub struct TraceRecord {
    pub level: TraceLevel,
    pub app: String,
    pub model_raw: String,
    pub in_fmt: String,
    pub target_fmt: String,
    pub trans_kind: String,
    pub upstream_status: u16,
    pub latency_ms: i64,
    pub retry_count: i64,
    pub stripped_fields: Vec<String>,
    pub content_length: Option<i64>,
    pub content_type: String,
    pub body_sha256: String,
    pub body_hex: String,
    pub upstream_error: String,
    pub note: String,
}

impl TraceRecord {
    /// 記錄請求體指紋與長度（**不存明文**）。
    pub fn with_body(mut self, body: &[u8]) -> Self {
        self.content_length = Some(body.len() as i64);
        self.body_sha256 = sha256_hex_bytes(body);
        self
    }

    /// 記錄請求體前綴 hex。僅供 body 解析失敗時使用，用於事後判定根因。
    pub fn with_body_hex(mut self, body: &[u8]) -> Self {
        self.body_hex = hex_prefix(body, BODY_HEX_MAX);
        self.with_body(body)
    }

    /// 記錄上游錯誤原文（自動截斷至 [`UPSTREAM_ERR_MAX`]）並升為 warn。
    pub fn with_upstream_error(mut self, err: &str) -> Self {
        self.upstream_error = truncate_chars(err, UPSTREAM_ERR_MAX);
        self.level = TraceLevel::Warn;
        self
    }

    /// 標為 warn（用於解析失敗等非上游錯誤的異常）。
    pub fn warn(mut self) -> Self {
        self.level = TraceLevel::Warn;
        self
    }
}
