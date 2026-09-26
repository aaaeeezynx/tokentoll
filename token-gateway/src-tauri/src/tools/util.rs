//! 共用小工具（首頁路徑、PATH 查找、網關判定）
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

pub fn user_home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "無法解析用戶主目錄".to_string())
}


pub(crate) fn on_path(names: &[&str]) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    // Windows 上可執行檔可能帶副檔名，額外探測常見位置。
    let _ = &mut dirs;
    dirs.iter().any(|d| {
        names.iter().any(|n| {
            let p = d.join(n);
            if p.is_file() {
                return true;
            }
            #[cfg(windows)]
            {
                for ext in ["exe", "cmd", "bat", "ps1"] {
                    if d.join(format!("{n}.{ext}")).is_file() {
                        return true;
                    }
                }
            }
            false
        })
    })
}


pub(crate) fn read_text(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}


pub(crate) fn strip_quotes(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"'))
            || (t.starts_with('\'') && t.ends_with('\'')))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}


pub(crate) fn is_gateway(url: &str, port: u16) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|u| {
        u.scheme() == "http"
            && u.host_str() == Some(GATEWAY_HOST)
            && u.port() == Some(port)
    })
}
