//! 更新檢查的實際邏輯（抓 manifest、比版本）。

use serde::Deserialize;

use super::{URL_KEY, LAST_KEY, LAST_RESULT_KEY};

/// Tauri v2 靜態 manifest 的形狀（只解析我們用得到的欄位）。
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct Manifest {
    /// 刻意給 `default`：缺 version 的 manifest 應該得到「沒有 version」
    /// 這種人話，而不是 serde 的 `missing field` ✗
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default, rename = "pub_date")]
    pub pub_date: String,
    #[serde(default)]
    pub platforms: std::collections::BTreeMap<String, PlatformEntry>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct PlatformEntry {
    #[serde(default)]
    pub signature: String,
    pub url: String,
}

/// 檢查結果（給前端顯示）。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    /// 有沒有比現在的新
    pub has_update: bool,
    pub notes: String,
    pub pub_date: String,
    /// 對應這個平台的下載網址（沒有＝manifest 沒放這個平台）
    pub url: String,
    /// manifest 有沒有附簽章（ Tauri 自動安裝需要；我們只提示）
    pub signed: bool,
    /// 檢查完的說明（給 UI 直接顯示）
    pub message: String,
}

/// 目前執行檔的版本。
pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 這個平台的 manifest key（Tauri 用 `windows-x86_64` 這種寫法）。
pub fn platform_key() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    if cfg!(target_os = "windows") {
        format!("windows-{arch}")
    } else if cfg!(target_os = "macos") {
        format!("darwin-{arch}")
    } else {
        format!("linux-{arch}")
    }
}

/// 版本比較：`a > b` 回正數。容忍前綴 `v` 與不同段數（`0.2` vs `0.2.0` 視為相同）。
///
/// 刻意**不比 pre-release**（`1.0.0-beta`）：我們的版本沒有那種格式，
/// 與其猜語意，不如讓 `1.0.0-beta` 的數字部分參與比較 —— 這點寫在文件裡。
pub fn compare_version(a: &str, b: &str) -> i32 {
    fn clean(s: &str) -> &str {
        s.trim()
            .trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("")
    }
    fn nums(s: &str) -> Vec<u64> {
        clean(s)
            .split('.')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (x, y) = (nums(a), nums(b));
    for i in 0..x.len().max(y.len()) {
        let xa = x.get(i).copied().unwrap_or(0);
        let ya = y.get(i).copied().unwrap_or(0);
        match xa.cmp(&ya) {
            std::cmp::Ordering::Less => return -1,
            std::cmp::Ordering::Greater => return 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    0
}

/// 解析 manifest 文字。
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let m: Manifest = serde_json::from_str(text).map_err(|e| format!("更新資訊格式不對：{e}"))?;
    if m.version.trim().is_empty() {
        return Err("更新資訊裡沒有 version".to_string());
    }
    Ok(m)
}

/// 比對 manifest 與目前版本，組成檢查結果。
pub fn build_result(manifest: &Manifest) -> UpdateCheck {
    let current = current_version();
    let key = platform_key();
    // 找不到完全對應的平台時，接受任何一個（大小寫不拘）—— manifest 寫錯平台名
    // 不該讓使用者卡住，但要讓他看得出來。
    let entry = manifest
        .platforms
        .get(&key)
        .or_else(|| {
            manifest
                .platforms
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&key))
                .map(|(_, v)| v)
        })
        .cloned();
    let has_update = compare_version(&manifest.version, &current) > 0;
    let message = match (&entry, has_update) {
        (Some(e), true) => format!(
            "有新版 {}（目前 {current}）；{}",
            manifest.version,
            if e.signature.is_empty() {
                "manifest 沒有附簽章，無法自動安裝"
            } else {
                "已附簽章"
            }
        ),
        (Some(_), false) => format!("已是最新（{current}）"),
        (None, true) => format!(
            "manifest 沒有 {} 這個平台的下載檔（看得到版本，但裝不了）",
            key
        ),
        (None, false) => format!("已是最新（{current}）"),
    };
    UpdateCheck {
        current,
        latest: manifest.version.clone(),
        has_update,
        notes: manifest.notes.clone(),
        pub_date: manifest.pub_date.clone(),
        url: entry.as_ref().map(|e| e.url.clone()).unwrap_or_default(),
        signed: entry.map(|e| !e.signature.is_empty()).unwrap_or(false),
        message,
    }
}

/// 抓 manifest 並檢查（HTTP 在呼叫端，這裡只處理文字）。
pub fn check_text(text: &str) -> Result<UpdateCheck, String> {
    let m = parse_manifest(text)?;
    Ok(build_result(&m))
}

/// 設定檔存取的小幫手（給命令層用）。
pub(crate) fn url_setting(conn: &rusqlite::Connection) -> String {
    crate::db::get_setting(conn, URL_KEY).unwrap_or_default().trim().to_string()
}

pub(crate) fn record(conn: &rusqlite::Connection, message: &str) -> Result<(), String> {
    crate::db::set_setting(conn, LAST_KEY, &crate::fsutil::now_ms().to_string())
        .map_err(|e| e.to_string())?;
    crate::db::set_setting(conn, LAST_RESULT_KEY, message).map_err(|e| e.to_string())
}

pub(crate) fn last_result(conn: &rusqlite::Connection) -> (i64, String) {
    let ms = crate::db::get_setting(conn, LAST_KEY)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (ms, crate::db::get_setting(conn, LAST_RESULT_KEY).unwrap_or_default())
}
