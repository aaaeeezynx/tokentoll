//! 額度資料結構：對外回傳的窗口與訂閱額度物件。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct QuotaWindow {
    /// 窗口顯示名（如「5 小時」「本週」「本月」）。
    pub label: String,
    /// 已用百分比 0-100。
    pub percent: f64,
    /// 重置時間（RFC3339；上游未給或零用量佔位時為 None）。
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionQuota {
    pub key: String,
    pub title: String,
    /// opencode-go | claude | codex
    pub kind: String,
    /// ok | no_credentials | expired | error | not_subscribed
    pub status: String,
    pub message: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub queried_at: i64,
}

impl SubscriptionQuota {
    pub(super) fn ok(key: &str, title: &str, kind: &str, windows: Vec<QuotaWindow>) -> Self {
        Self {
            key: key.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            status: "ok".to_string(),
            message: None,
            windows,
            queried_at: crate::fsutil::now_ms(),
        }
    }

    pub(super) fn fail(key: &str, title: &str, kind: &str, status: &str, message: impl Into<String>) -> Self {
        Self {
            key: key.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            status: status.to_string(),
            message: Some(message.into()),
            windows: vec![],
            queried_at: crate::fsutil::now_ms(),
        }
    }

    pub(super) fn no_credentials(key: &str, title: &str, kind: &str, message: impl Into<String>) -> Self {
        Self::fail(key, title, kind, "no_credentials", message)
    }
}
