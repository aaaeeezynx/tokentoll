pub mod commands;
pub mod error;
mod db;
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
mod tools;
mod trace;
mod translate;
mod tray;
mod usage;
mod usage_query;

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

            let show = MenuItem::with_id(app, "show", "顯示主視窗", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
            // 啟動時先放一個最小選單；tray::spawn 會立刻用資料庫內容重建
            // （之後每 30 秒重建一次，來源變動不必重啟 App）。
            let menu = Menu::with_items(app, &[&show, &sep, &quit])?;
            tray::spawn(app.handle(), menu)?;
            tray::rebuild(app.handle()).map_err(std::io::Error::other)?;
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
