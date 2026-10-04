//! 視窗與托盤行為（P4.7，對標 cc-switch 的「關閉行為」與 Lightweight Mode）。
//!
//! ## cc-switch 的實際行為（照它手冊 §1.5 抄）
//!
//! - 「**Minimize to tray on close**」**預設開啟**：按視窗的關閉鈕只是縮到系統匣，
//!   App 繼續在背景跑；關掉這個選項才會真的結束。
//! - 「**Lightweight Mode**」：右鍵托盤 → 進入後**視窗被銷毀**（不只是隱藏），
//!   從托盤的「Open main window」或 Deep Link 再叫回來，**狀態不持久**
//!   （下次正常啟動回到一般模式）。
//!
//! ## 為什麼我們特別需要它
//!
//! 這是**本地網關**：使用者的 CLI 工具（Claude Code／Codex／OpenCode）流量都經過
//! 它。如果按個關閉鈕就把行程結束掉，工具會突然連不上上游 —— 原本的行為就是這樣
//! （沒有攔 `CloseRequested`）。所以預設改成縮到系統匣，與 cc-switch 一致，
//! 也符合這個 App 的用途。
//!
//! ## 狀態怎麼判斷
//!
//! 輕量模式**不另外記一個旗標**：`get_webview_window("main")` 回 `None` 就是
//! 處於輕量模式。少一個會與事實不同步的狀態（旗標忘了清掉就會出現「以為有視窗
//! 但其實沒有」的鬼故事）。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 關閉視窗時的行為（設定鍵 `close_action`）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseAction {
    /// 縮到系統匣，行程繼續（**預設**，與 cc-switch 相同）
    Tray,
    /// 直接結束
    Exit,
}

impl CloseAction {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "exit" => CloseAction::Exit,
            _ => CloseAction::Tray,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CloseAction::Tray => "tray",
            CloseAction::Exit => "exit",
        }
    }
}

/// 讀設定（預設縮到系統匣）。
pub fn close_action(conn: &rusqlite::Connection) -> CloseAction {
    CloseAction::parse(&crate::db::get_setting(conn, "close_action").unwrap_or_default())
}

/// 現在是不是處於輕量模式（沒有視窗）。
pub fn is_lightweight(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_none()
}

/// 建立主視窗（與 `tauri.conf.json` 的設定一致：1100×720、最小 600×480）。
fn build_main(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Token Toll")
        .inner_size(1100.0, 720.0)
        .min_inner_size(600.0, 480.0)
        .build()
        .map_err(|e| format!("建立主視窗失敗：{e}"))
}

/// 顯示主視窗；**視窗不存在就重建**（從輕量模式回來，或 Deep Link 喚醒）。
pub fn show_main(app: &AppHandle) -> Result<(), String> {
    match app.get_webview_window("main") {
        Some(w) => {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            Ok(())
        }
        None => {
            let w = build_main(app)?;
            let _ = w.show();
            let _ = w.set_focus();
            Ok(())
        }
    }
}

/// 進入輕量模式：銷毀視窗（行程與網關都留著）。
pub fn enter_lightweight(app: &AppHandle) -> Result<(), String> {
    match app.get_webview_window("main") {
        Some(w) => w.destroy().map_err(|e| format!("關閉視窗失敗：{e}")),
        None => Ok(()), // 已經是輕量模式
    }
}

/// 攔截關閉事件時要做的決定。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OnClose {
    /// 攔下來並隱藏（縮到系統匣）
    Hide,
    /// 放行（真的結束）
    Allow,
}

/// 依設定決定關閉鈕該做什麼（純函式，方便測試）。
pub fn decide_close(action: CloseAction) -> OnClose {
    match action {
        CloseAction::Tray => OnClose::Hide,
        CloseAction::Exit => OnClose::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_action_defaults_to_tray() {
        assert_eq!(CloseAction::parse(""), CloseAction::Tray, "預設與 cc-switch 相同");
        assert_eq!(CloseAction::parse("TRAY"), CloseAction::Tray);
        assert_eq!(CloseAction::parse(" exit "), CloseAction::Exit);
        // 亂填的值一律回安全的那一邊（行程活著總比被關掉好救）
        assert_eq!(CloseAction::parse("nonsense"), CloseAction::Tray);
        assert_eq!(CloseAction::Tray.as_str(), "tray");
        assert_eq!(CloseAction::Exit.as_str(), "exit");
    }

    #[test]
    fn decide_close_matches_the_action() {
        assert_eq!(decide_close(CloseAction::Tray), OnClose::Hide);
        assert_eq!(decide_close(CloseAction::Exit), OnClose::Allow);
    }
}
