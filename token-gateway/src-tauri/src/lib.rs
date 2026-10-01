pub mod cloudsync;
mod commands;
pub mod error;
mod autostart;
mod db;
mod dbbackup;
mod deeplink;
#[allow(dead_code)]
mod fsutil;
mod history;
mod keys;
mod mcp;
mod models;
mod price_extract;
mod presets;
mod prompt;
mod provider_check;
mod providers;
mod providers_io;
mod sessions;
mod quota;
#[doc(hidden)]
pub mod proxy;
mod skills;
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

/// Deep Link 的啟動處理。
///
/// - 命令列有連結、而且**已經有實例在跑**（閘道埠被佔用）→ 把連結寫進收件匣、
///   結束這個新行程（避免兩個行程搶同一個埠與資料庫）。
/// - 否則：確保 `tokengateway://` 已註冊（HKCU，免管理員），並把連結回傳給前端。
fn deeplink_startup(app: &tauri::AppHandle) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let link = deeplink::find_link(&args);
    if let Some(url) = &link {
        if deeplink::protocol::another_instance_running() {
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let _ = deeplink::protocol::write_inbox(&dir, url);
            std::process::exit(0);
        }
    }
    // 註冊協定（失敗不影響啟動 —— 例如被群組原則擋掉）
    if let Ok(exe) = std::env::current_exe() {
        let _ = deeplink::protocol::ensure_registered(&exe);
    }
    link
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 資料庫 fail-fast：初始化失敗直接阻斷啟動，避免帶病執行。
            let state = db::init_db(app.handle()).expect("failed to init database");
            app.manage(state);
            app.manage(proxy::ProxyState::default());

            // Deep Link（P4.1）：解析啟動參數 → 已經有實例在跑就轉交後結束；
            // 否則留在 pending 讓前端掛載時取走，並註冊 tokengateway:// 協定。
            let startup_link = deeplink_startup(app.handle());
            app.manage(deeplink::DeeplinkState(std::sync::Mutex::new(startup_link)));
            deeplink::protocol::spawn_watcher(app.handle())?;
            // 自動備份排程（P4.8）：每 5 分鐘檢查一次「該不該備份」
            dbbackup::spawn_scheduler(app.handle().clone());

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
            // MCP 管理（P3.1，見 mcp/）
            commands::mcp_list,
            commands::mcp_presets,
            commands::mcp_upsert,
            commands::mcp_delete,
            commands::mcp_set_binding,
            commands::mcp_set_app_all,
            commands::mcp_sync,
            commands::mcp_import,
            // 提示詞預設集（P3.2，見 prompt/）
            commands::prompt_apps,
            commands::prompt_state,
            commands::prompt_list,
            commands::prompt_save,
            commands::prompt_activate,
            commands::prompt_deactivate,
            commands::prompt_delete,
            commands::prompt_sync,
            commands::prompt_live,
            // 技能管理（P3.3，見 skills/）
            commands::skills_repos,
            commands::skills_repo_add,
            commands::skills_repo_delete,
            commands::skills_discover,
            commands::skills_install,
            commands::skills_list,
            commands::skills_set_binding,
            commands::skills_update,
            commands::skills_update_all,
            commands::skills_uninstall,
            commands::skills_backups,
            commands::skills_restore,
            commands::skills_backup_delete,
            commands::skills_settings,
            // 會話管理（P3.4，見 sessions/）
            commands::sessions_apps,
            commands::sessions_scan,
            commands::sessions_read,
            commands::sessions_delete,
            // Deep Link（P4.1，見 deeplink/）
            commands::deeplink_take_pending,
            commands::deeplink_preview,
            commands::deeplink_apply,
            commands::deeplink_protocol_state,
            commands::deeplink_register,
            commands::deeplink_unregister,
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
            // 資料庫備份管理（P4.8）
            commands::db_backup_state,
            commands::db_backup_now,
            commands::db_backup_restore,
            commands::db_backup_rename,
            commands::db_backup_delete,
            commands::db_backup_set_schedule,
            // 雲端同步（P4.10，見 cloudsync/）
            commands::cloud_state,
            commands::cloud_set_config,
            commands::cloud_set_auto,
            commands::cloud_test,
            commands::cloud_remote_info,
            commands::cloud_upload,
            commands::cloud_download,
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
