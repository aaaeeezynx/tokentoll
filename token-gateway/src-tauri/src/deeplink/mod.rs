//! Deep Link（P4.1，對標 cc-switch 的 `ccswitch://` 一鍵匯入）。
//!
//! 使用者點一個 `tokengateway://v1/import?…` 連結 → App 打開（或已經開著）→
//! **顯示匯入確認對話框**（型別、內容預覽、安全提醒）→ 使用者確認才寫入。
//!
//! 三個部分：
//! - [`parse`]：解析＋驗證，產生**不含完整金鑰**的預覽。
//! - [`apply`]：真正寫入（沿用各模組既有的 store，不另開寫入路徑）。
//! - [`protocol`]：在 Windows 登錄檔註冊 `tokengateway://`（**HKCU**，
//!   不需要系統管理員），以及「已經開著時」用收件匣檔案轉交。
//!
//! cc-switch 靠安裝程式註冊協定；我們改成**首次啟動時自己註冊到 HKCU**
//! （安裝程式是 Tauri 產生的，改它不如自己寫一小段，而且 HKCU 免管理員權限）。

pub(crate) mod apply;
pub(crate) mod parse;
pub(crate) mod protocol;

pub use apply::{apply, Applied};
pub use parse::{parse, preview, DeeplinkPreview};

/// 協定名稱（cc-switch 是 `ccswitch`）。
pub const SCHEME: &str = "tokengateway";

/// 啟動時從命令列拿到、還沒被前端取走的連結。
///
/// 前端掛載時呼叫 `deeplink_take_pending` 取走；之後的連結走 `deeplink` 事件。
#[derive(Default)]
pub struct DeeplinkState(pub std::sync::Mutex<Option<String>>);

/// 從一行命令列參數裡挑出 deep link。
pub fn find_link(argv: &[String]) -> Option<String> {
    argv.iter()
        .find(|a| a.starts_with(&format!("{SCHEME}://")))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_scan_picks_the_link() {
        let argv = vec![
            "C:/app/token-gateway.exe".to_string(),
            "--flag".to_string(),
            "tokengateway://v1/import?resource=provider&app=claude&name=x".to_string(),
        ];
        assert_eq!(
            find_link(&argv).unwrap(),
            "tokengateway://v1/import?resource=provider&app=claude&name=x"
        );
        assert!(find_link(&["x".to_string()]).is_none());
        // 別的 scheme 不算
        assert!(find_link(&["ccswitch://v1/import?resource=provider".to_string()]).is_none());
    }

    #[test]
    fn scheme_constant_matches_the_parser() {
        assert_eq!(SCHEME, "tokengateway");
        assert!(parse("tokengateway://v1/import?resource=provider&app=claude&name=x").is_ok());
    }
}
