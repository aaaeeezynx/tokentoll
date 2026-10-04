//! 來源連線檢查（對標 CC Switch 的 "Connectivity check"）。
//!
//! ## 語意刻意與 CC Switch 一致
//!
//! **只檢查位址可不可達，不送真正的模型請求。** 所以 401／403／404 都算「可達」——
//! 它們證明主機與路徑活著，只是沒有金鑰（或那個路徑本來就沒有端點）。
//! 「金鑰與模型名對不對」是另一件事，只有真的發請求才知道；CC Switch 的 FAQ
//! 也特別提醒使用者「連線檢查過了不代表請求會成功」，這裡照同樣的定位做，
//! 訊息文字會把這一點講清楚，不讓使用者誤以為「檢查過了就一定能用」。
//!
//! ## 為什麼不用 HEAD
//!
//! 有些上游對 HEAD 回 405 或直接斷線。一律用 GET，並且**不帶任何鑑權標頭**
//! ——帶了金鑰就變成「用你的憑證打上游」，那是模型請求的範疇，不是連線檢查。

use serde::Serialize;
use std::time::Duration;

/// 連線階段上限（秒）：比網關的 10 秒短，因為這裡的使用者在等一個按鈕。
const CONNECT_TIMEOUT_SECS: u64 = 6;
/// 整體上限（秒）。
const TOTAL_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckResult {
    /// 實際檢查的位址（正規化後）
    pub url: String,
    /// 傳輸層是否成功（有拿到任何 HTTP 回應）
    pub reachable: bool,
    /// HTTP 狀態碼；0 = 連線階段就失敗
    pub status: u16,
    pub latency_ms: i64,
    /// 給人看的一句話
    pub message: String,
}

/// 正規化使用者填的 base_url：補 scheme、去掉結尾斜線與空白。
///
/// 空字串或看不出是網址的東西回 `None`（讓 UI 顯示「請先填寫上游地址」）。
pub(crate) fn normalize_url(base_url: &str) -> Option<String> {
    let s = base_url.trim();
    if s.is_empty() {
        return None;
    }
    let with_scheme = if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else if s.contains("://") {
        // 其他 scheme（ftp:// 之類）不是我們要的
        return None;
    } else if s.starts_with("localhost") || s.starts_with("127.0.0.1") {
        format!("http://{s}")
    } else {
        format!("https://{s}")
    };
    let trimmed = with_scheme.trim_end_matches('/').to_string();
    // 只有 scheme 沒有主機（例如 "https://"）→ 無效
    let rest = trimmed
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or_default();
    if rest.is_empty() || !rest.contains('.') && !rest.starts_with("localhost") && !rest.starts_with("127.0.0.1") {
        return None;
    }
    Some(trimmed)
}

/// 狀態碼 → (人話, 是否可達)。可達＝傳輸層成功，不代表金鑰或模型沒問題。
pub(crate) fn classify(status: u16) -> (String, bool) {
    match status {
        0 => ("連不上（DNS、逾時或被拒絕）".to_string(), false),
        200..=299 => (format!("可達（{status}）"), true),
        401 | 403 => (format!("可達（{status}：需要金鑰）"), true),
        404 | 405 | 501 => (format!("可達（{status}：這個路徑沒有端點，屬正常）"), true),
        429 => (format!("可達（{status}：被限流）"), true),
        400..=499 => (format!("可達（{status}）"), true),
        500..=599 => (
            format!("可達，但上游回 {status}（伺服器端問題）"),
            true,
        ),
        _ => (format!("可達（{status}）"), true),
    }
}

async fn check_with(client: &reqwest::Client, url: &str) -> CheckResult {
    let started = std::time::Instant::now();
    match client
        .get(url)
        .header("accept", "*/*")
        .header("user-agent", "tokentoll/connectivity-check")
        .send()
        .await
    {
        Ok(r) => {
            let status = r.status().as_u16();
            let (message, reachable) = classify(status);
            CheckResult {
                url: url.to_string(),
                reachable,
                status,
                latency_ms: started.elapsed().as_millis() as i64,
                message,
            }
        }
        Err(e) => CheckResult {
            url: url.to_string(),
            reachable: false,
            status: 0,
            latency_ms: started.elapsed().as_millis() as i64,
            message: format!("連不上：{e}"),
        },
    }
}

/// 檢查一個來源位址是否可達（**不帶金鑰、不送模型請求**）。
#[tauri::command]
pub async fn provider_check(base_url: String) -> Result<CheckResult, String> {
    let url = normalize_url(&base_url)
        .ok_or_else(|| "請先填寫有效的上游地址（例如 https://api.example.com/v1）".to_string())?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(TOTAL_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(check_with(&client, &url).await)
}

#[cfg(test)]
mod tests;
