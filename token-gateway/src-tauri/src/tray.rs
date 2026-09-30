//! 系統匣的「來源切換」選單（P1.6，對標 CC Switch 的托盤切換）。
//!
//! ## 職責切分（刻意如此）
//!
//! 這一層只做兩件事：**決定選單長什麼樣**（哪些工具、各有哪些可用來源）與
//! **把點擊變成一個事件**。真正「切換」的動作**不在 Rust 做**：
//!
//! 點下去之後 `lib.rs` 會 `emit("tray-switch", {app, provider_id})`，由前端既有的
//! `useToolSwitch` 走**同一條路**（挑模型：沿用上次用的、否則第一個啟用模型；
//! 挑 Key：上次的偏好；寫設定、備份、回報）。
//!
//! 為什麼不直接在 Rust 寫完？因為「上次用哪個 Key／那個模型還在不在」這些規則
//! 已經有一份實作在前端（`resolveSelectionReq`）。在 Rust 再寫一份＝製造第二個
//! 真相來源，兩邊遲早會不一致 —— 這一輪已經在其他地方吃過這個虧（見 §0.9.20）。
//!
//! ## 選單內容
//!
//! 只列**可用**的來源：啟用中、且至少有一個啟用模型的來源。列了卻點不動
//! （例如「這個來源沒有啟用的模型」）比不列更糟。
//!
//! 選單在啟動時建立，並且**每 30 秒重建一次** —— 使用者在 App 裡新增／改名／
//! 刪除來源之後，不必重啟就會出現在托盤上。

use rusqlite::Connection;
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::DbState;
use crate::tools::{APPS, TAKEOVER_APPS};

/// 托盤選單項目的 id 前綴。
const SW_PREFIX: &str = "sw:";
/// 被管理的托盤圖示 id（重建選單時要找回它）。
pub const TRAY_ID: &str = "main";
/// 選單重建間隔（秒）。
const REBUILD_SECS: u64 = 30;
/// 托盤切換事件的名稱。
///
/// 前端 `TraySwitchListener.tsx` 用**同一個字串**監聽（不同語言，沒有編譯器幫忙）。
/// 所以這裡把它變常數並用測試釘住；改名字一定要同時改兩邊，否則托盤點了會靜默沒反應。
pub const EVENT_SWITCH: &str = "tray-switch";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrayProvider {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrayTool {
    pub app: String,
    /// 顯示名（例如「Claude Code」）
    pub display: String,
    pub providers: Vec<TrayProvider>,
}

/// 組出選單模型：可接管工具 × 可用來源（依來源清單順序）。
pub fn menu_model(conn: &Connection) -> Result<Vec<TrayTool>, String> {
    let mut out = Vec::new();
    for app_id in TAKEOVER_APPS {
        let display = APPS
            .iter()
            .find(|(a, _)| *a == app_id)
            .map(|(_, d)| *d)
            .unwrap_or(app_id)
            .to_string();
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.name
                 FROM providers p
                 WHERE p.enabled = 1
                   AND EXISTS (SELECT 1 FROM provider_models m
                               WHERE m.provider_id = p.id AND m.enabled = 1)
                 ORDER BY p.priority ASC, p.id ASC",
            )
            .map_err(|e| e.to_string())?;
        let providers = stmt
            .query_map([], |r| {
                Ok(TrayProvider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        out.push(TrayTool {
            app: app_id.to_string(),
            display,
            providers,
        });
    }
    Ok(out)
}

/// 選單項目 id。
pub fn item_id(app: &str, provider_id: i64) -> String {
    format!("{SW_PREFIX}{app}:{provider_id}")
}

/// 解析選單項目 id；不是切換項就回 `None`（交給「顯示主視窗／退出」處理）。
///
/// 未知工具名一律拒絕 —— 免得選單被動手腳後把設定寫到不存在的工具上。
pub fn parse_item_id(id: &str) -> Option<(String, i64)> {
    let rest = id.strip_prefix(SW_PREFIX)?;
    let (app, pid) = rest.split_once(':')?;
    if !TAKEOVER_APPS.contains(&app) {
        return None;
    }
    let provider_id: i64 = pid.parse().ok()?;
    if provider_id <= 0 {
        return None;
    }
    Some((app.to_string(), provider_id))
}

/// 建立（或重建）托盤選單。
pub fn rebuild(app: &AppHandle) -> Result<(), String> {
    let model = {
        let db = app.state::<DbState>();
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        menu_model(&conn)?
    };
    let show = MenuItem::with_id(app, "show", "開啟主視窗", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    // 輕量模式（cc-switch 的 Lightweight Mode）：銷毀視窗、只留托盤。
    // 已經在輕量模式時把它變灰，避免使用者以為點了沒反應。
    let light = MenuItem::with_id(
        app,
        "lightweight",
        "輕量模式（關閉視窗）",
        !crate::window::is_lightweight(app),
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep1 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let sep2 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let sep3 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    // 每個工具一個子選單，裡面是它的可用來源。
    let mut subs: Vec<Submenu<tauri::Wry>> = Vec::new();
    for t in &model {
        let mut items: Vec<MenuItem<tauri::Wry>> = Vec::new();
        if t.providers.is_empty() {
            items.push(
                MenuItem::with_id(
                    app,
                    format!("empty:{}", t.app),
                    "（沒有可用的來源）",
                    false,
                    None::<&str>,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        for p in &t.providers {
            items.push(
                MenuItem::with_id(app, item_id(&t.app, p.id), &p.name, true, None::<&str>)
                    .map_err(|e| e.to_string())?,
            );
        }
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
            .collect();
        subs.push(
            Submenu::with_items(app, format!("切換{}", t.display), true, &refs)
                .map_err(|e| e.to_string())?,
        );
    }

    let mut all: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&show, &sep1];
    for s in &subs {
        all.push(s);
    }
    all.push(&sep2);
    all.push(&light);
    all.push(&sep3);
    all.push(&quit);
    let menu = Menu::with_items(app, &all).map_err(|e| e.to_string())?;
    match app.tray_by_id(TRAY_ID) {
        Some(tray) => tray.set_menu(Some(menu)).map_err(|e| e.to_string()),
        None => Ok(()), // 托盤還沒建立（啟動階段的第一次）
    }
}

/// 啟動時建立托盤，之後每 30 秒重建一次選單（來源變動不必重啟）。
pub fn spawn(app: &AppHandle, menu: Menu<tauri::Wry>) -> tauri::Result<()> {
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("missing default window icon");
    tauri::tray::TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Token Gateway · 本地網關")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some((target_app, provider_id)) = parse_item_id(id) {
                // 只發事件，實際切換由前端既有的流程做（見本檔開頭的職責說明）。
                let payload = serde_json::json!({
                    "app": target_app,
                    "provider_id": provider_id,
                });
                if let Err(e) = app.emit(EVENT_SWITCH, payload) {
                    eprintln!("tray: 發送切換事件失敗: {e}");
                }
                return;
            }
            match id {
                "show" => {
                    if let Err(e) = crate::window::show_main(app) {
                        eprintln!("tray: 開啟主視窗失敗: {e}");
                    }
                }
                "lightweight" => {
                    if let Err(e) = crate::window::enter_lightweight(app) {
                        eprintln!("tray: 進入輕量模式失敗: {e}");
                    } else if let Err(e) = rebuild(app) {
                        eprintln!("tray: 重建選單失敗: {e}");
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;

    // 定期重建：使用者在 App 裡新增／改名／刪除來源後，托盤不必重啟就會更新。
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(REBUILD_SECS));
        if let Err(e) = rebuild(&handle) {
            eprintln!("tray: 重建選單失敗: {e}");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
