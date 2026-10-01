mod commands;
pub mod error;
mod autostart;
mod db;
#[allow(dead_code)]
mod fsutil;
mod history;
mod keys;
mod models;
mod price_extract;
mod presets;
mod provider_check;
mod providers;
mod providers_io;
mod quota;
#[doc(hidden)]
pub mod proxy;
mod theme;
mod tools;
mod trace;
mod translate;
mod tray;
mod usage;
mod usage_query;
mod window;

use tauri::menu::{Menu, MenuItem};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 資料庫 fail-fast：初始化失敗直接阻斷啟動，避免帶病執行。
            let state = db::init_db(app.handle()).expect("failed to init database");
            app.manage(state);
            app.manage(proxy::ProxyState::default());

            let show = MenuItem::with_id(app, "show", "開啟主視窗", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
            // 啟動時先放一個最小選單；tray::spawn 會立刻用資料庫內容重建
            // （之後每 30 秒重建一次，來源變動不必重啟 App）。
            let menu = Menu::with_items(app, &[&show, &sep, &quit])?;
            tray::spawn(app.handle(), menu)?;
            tray::rebuild(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        // 關閉視窗時的行為（P4.7）：預設縮到系統匣，行程與網關繼續跑
        // （這是本地網關，按個 X 就斷掉所有 CLI 工具的流量是很糟的體驗）。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 用 clone 出來的 AppHandle 取狀態：State 的借用掛在 manager 上，
                // 直接寫 `window.state::<…>()` 的話借用活不夠久（E0597）。
                let handle = window.app_handle().clone();
                let action = {
                    let db = handle.state::<db::DbState>();
                    // 把 guard 綁成區域變數：直接寫在 match 的臨時值會在 db 之後才 drop
                    let guard = db.conn.lock();
                    match guard {
                        Ok(conn) => window::close_action(&conn),
                        Err(_) => window::CloseAction::Tray,
                    }
                };
                match window::decide_close(action) {
                    window::OnClose::Hide => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    window::OnClose::Allow => {
                        // 光放行還不夠：**有托盤圖示時，關掉最後一個視窗不會結束行程**
                        // （事件迴圈被托盤撐著）。實機驗證就是這樣卡住的 —— 設定成
                        // 「直接結束」卻只變成「沒有視窗的托盤模式」。所以明講結束。
                        api.prevent_close();
                        handle.exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::db_status,
            commands::proxy_status,
            commands::proxy_check_port,
            commands::proxy_start,
            commands::proxy_stop,
            commands::settings_get,
            commands::settings_set,
            commands::settings_set_accent,
            commands::keys_list,
            commands::key_create,
            commands::key_update,
            commands::key_set_enabled,
            commands::key_delete,
            commands::key_rotate,
            commands::key_reveal,
            commands::recent_logs,
            commands::history_scan,
            commands::history_import,
            commands::usage_summary,
            commands::usage_trend,
            commands::usage_trend_by_app,
            commands::usage_export_csv,
            commands::price_quote,
            commands::subscription_fees,
            commands::usage_by_provider,
            commands::usage_by_model,
            commands::usage_by_app,
            commands::usage_hourly,
            commands::pricing_list,
            commands::pricing_upsert,
            commands::pricing_delete,
            commands::pricing_clear,
            commands::pp_list,
            commands::pp_upsert,
            commands::pp_delete,
            commands::pp_periods,
            commands::pp_period_add,
            commands::pp_period_update,
            commands::pp_period_delete,
            commands::app_data_dir,
            commands::tools_detect,
            commands::tool_versions,
            commands::tool_latest,
            commands::tool_update,
            commands::switch_plan,
            commands::switch_apply,
            commands::switch_restore,
            commands::switch_off,
            commands::codex_doctor,
            commands::catalog_fetch,
            commands::catalog_get,
            commands::models_list,
            commands::model_counts,
            commands::model_add,
            commands::model_update,
            commands::model_delete,
            providers::providers_list,
            providers::provider_add,
            providers::provider_update,
            providers::provider_delete,
            providers::provider_reorder,
            // 來源預設集與複製／匯入匯出（對齊 CC Switch）
            presets::presets_list,
            providers_io::provider_duplicate,
            providers_io::providers_export,
            providers_io::providers_export_file,
            providers_io::providers_import,
            providers_io::providers_import_file,
            commands::price_extract,
            commands::quota_query_all,
            // Phase 1：診斷中心（可觀測性，見 docs/REFACTORING-PLAN.md §5.3）
            commands::trace_summary,
            commands::trace_list,
            commands::trace_problems,
            commands::trace_clear,
            commands::provider_stripped_list,
            commands::provider_stripped_all,
            commands::provider_stripped_clear,
            // 來源健康狀態（斷路器，見 proxy/health.rs）
            commands::source_health,
            // 故障轉移佇列預覽（P1.4，見 proxy/queue.rs）
            commands::failover_models,
            commands::failover_queue,
            // 來源連線檢查（P1.5，見 provider_check.rs）
            provider_check::provider_check,
            // 每來源的用量查詢（P2.1，見 usage_query/）
            commands::usage_query_get,
            commands::usage_query_set,
            commands::usage_query_clear,
            commands::usage_query_templates,
            commands::usage_query_apply_template,
            commands::usage_query_run,
            commands::usage_query_run_all,
            // 外觀與啟動（P4.3／P4.4）
            commands::appearance_state,
            commands::appearance_set_theme,
            commands::appearance_set_lang,
            commands::appearance_set_autostart,
            // 視窗與托盤行為（P4.7）
            commands::window_behavior,
            commands::window_set_close_action,
            commands::window_enter_lightweight,
            commands::window_show_main,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // 托盤常駐（P4.7）：**沒有視窗也要活著**。
            //
            // Tauri 的預設是「所有視窗都沒了就結束行程」，所以進入輕量模式
            // （銷毀視窗）會把整個 App 連網關一起關掉 —— 實機驗證就是這樣踩到的。
            // 這裡把「因為沒有視窗而想結束」擋下來；真正的退出（托盤的「退出」、
            // 或關閉行為設成 exit）走的是 `app.exit(0)`，那個 `code` 是 `Some(0)`，
            // 所以放行 —— 用 `code` 區分「視窗關光」與「使用者真的要退出」。
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
