pub mod commands;
pub mod error;
mod db;
#[allow(dead_code)]
mod fsutil;
mod history;
mod keys;
mod models;
mod price_extract;
mod providers;
mod quota;
#[doc(hidden)]
pub mod proxy;
mod tools;
mod trace;
mod translate;
mod usage;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
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

            let show = MenuItem::with_id(app, "show", "顯示主視窗", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .expect("missing default window icon");
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Token Gateway · 本地網關未執行")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
