//! tools 相關測試的入口。原本 1,524 行全在同一個 `mod tests` 裡，已依主題
//! 拆成五個子模組；共用 helper 留在這裡。
//!
//! 子模組的 `use super::*;` 只會帶進**本模組自己的**綁定：看得到父模組的私有
//! 項目與 `pub(crate) use` 再匯出，但看不到父模組的 `use` 別名。因此這裡把子模組
//! 實際用到的符號逐一再匯出（清單由掃描子模組原始碼產生，非手寫維護）。

use crate::tools::backup::{restore_backup_port, restore_backup_to_inner};

pub(crate) use std::path::Path;

// ---- 子模組需要的再匯出 ----

// ---- backup ----
pub(crate) use crate::tools::backup::backup_is_tainted;
pub(crate) use crate::tools::backup::ensure_baseline_before_prune;
pub(crate) use crate::tools::backup::has_baseline;
pub(crate) use crate::tools::backup::prune_backups_keep_clean;
pub(crate) use crate::tools::backup::restore_backup_to_port;
pub(crate) use crate::tools::backup::write_baseline;

// ---- codex ----
pub(crate) use crate::tools::codex::CodexAuth;
pub(crate) use crate::tools::codex::LegacyProviders;
pub(crate) use crate::tools::codex::codex_alias_ids;
pub(crate) use crate::tools::codex::codex_apply;
pub(crate) use crate::tools::codex::codex_doctor_at;
pub(crate) use crate::tools::codex::codex_legacy_providers_report;
pub(crate) use crate::tools::codex::codex_wire_api;
pub(crate) use crate::tools::codex::codex_wire_api_declared;

// ---- consts ----
pub(crate) use crate::tools::consts::BACKUP_KEEP;
pub(crate) use crate::tools::consts::CODEX_SHARED_PROVIDER_ID;
pub(crate) use crate::tools::consts::GATEWAY_PROVIDER_ID;

// ---- detect ----
pub(crate) use crate::tools::detect::detect_tools;

// ---- switch ----
pub(crate) use crate::tools::switch::SwitchRequest;
pub(crate) use crate::tools::switch::apply_switch;
pub(crate) use crate::tools::switch::plan_switch;

// ---- versions ----
pub(crate) use crate::tools::versions::clean_version;
pub(crate) use crate::tools::versions::cmp_version;
pub(crate) use crate::tools::versions::npm_pkg_version_at;
pub(crate) use crate::tools::versions::run_with_timeout;
pub(crate) use crate::tools::versions::tool_latest;
pub(crate) use crate::tools::versions::tool_versions;

// ---- apply ----
pub(crate) use crate::tools::apply::ClaudeModelMap;
pub(crate) use crate::tools::apply::claude_apply;
pub(crate) use crate::tools::apply::config_path_for;
pub(crate) use crate::tools::apply::opencode_apply;

/// 測試便利包裝：以預設埠呼叫正式還原路徑（migrate=true）。
///
/// 這兩個函式原本是生產碼（`restore_backup_to` / `restore_backup`）。
/// 它們只被測試呼叫，因此在 lib 建置下恆為 dead_code（B7）。
/// `restore_backup` 是 `restore_backup_port` 的預設埠等價包裝，直接對應；
/// `restore_backup_to` 則不同 —— 它傳 `migrate: false`，
/// 是「不做 baseline 遷移」的還原路徑，測試需要精確覆蓋它，
/// 故這裡照抄原行為而非改走 `restore_backup_to_port`。
fn restore_backup_to(app_data: &Path, app: &str, cfg: &Path) -> Result<String, String> {
    restore_backup_to_inner(app_data, app, cfg, crate::db::DEFAULT_GATEWAY_PORT, false)
}

fn restore_backup(app_data: &Path, app: &str) -> Result<String, String> {
    restore_backup_port(app_data, app, crate::db::DEFAULT_GATEWAY_PORT)
}

// ---- live 測試共用 helper（原在 `mod tests` 內，被多個 live_* 測試共用）----
// 子模組 `use super::*;` 帶得進這裡的**私有**項目，故不需再匯出。

/// 真實接管（默认忽略，需显式 `-- --ignored live_codex_takeover` 執行）：
/// 對真實 ~/.codex/config.toml 做一次網關接管（默認 NIM 渠道 15）。
/// 按需改 LIVE_* 常量：LIVE_PID（來源 id；16 等已刪 id 用於驗證守衛攔截）、
/// LIVE_MODEL、LIVE_DIRECT（true = 直連上游 Key，不經本地 Key）。
/// 前置：Codex 完全退出；TOKEN_GATEWAY_KEY 已設為用戶環境變量。
const LIVE_PID: i64 = 15;

const LIVE_MODEL: &str = "deepseek-ai/deepseek-v4-flash-0731";

const LIVE_DIRECT: bool = true;

fn live_dirs() -> (std::path::PathBuf, std::path::PathBuf) {
    let home = dirs::home_dir().expect("home");
    let app_data = home
        .join("AppData")
        .join("Roaming")
        .join("com.tokencounter.gateway");
    (home, app_data)
}

/// 復刻命令層直連注入：只讀 DB 取來源上游 Key。
fn live_upstream(app_data: &std::path::Path, pid: i64) -> String {
    let conn =
        rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
    let up: String = conn
        .query_row(
            "SELECT api_key FROM providers WHERE id=?1 AND enabled=1",
            rusqlite::params![pid],
            |r| r.get(0),
        )
        .expect("upstream key");
    assert!(!up.trim().is_empty(), "來源 {pid} 未填寫上游 Key");
    up
}

/// 復刻前端開關 ON 的請求（經網關；claude 填上游 Key 明文，
/// codex／opencode 空 Key 走 direct_upstream 由後端注入）。
fn live_req(app: &str, base_url: &str, key: String, direct: bool) -> SwitchRequest {
    SwitchRequest {
        app: app.into(),
        base_url: base_url.into(),
        api_key: key,
        model: LIVE_MODEL.into(),
        provider_id: Some(LIVE_PID),
        provider_format: None,
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: direct,
        key_id: None,
        claude_map: None,
    }
}

fn live_bak_names(app_data: &std::path::Path, app: &str) -> Vec<String> {
    let dir = app_data.join("backups").join(app);
    let mut v: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// 刪測試期新增的備份（以前後名單差集判定），讓備份目錄回到測試前。
fn live_rm_new_baks(app_data: &std::path::Path, app: &str, before: &[String]) {
    let dir = app_data.join("backups").join(app);
    for n in live_bak_names(app_data, app) {
        if !before.contains(&n) {
            let _ = std::fs::remove_file(dir.join(n));
        }
    }
}

mod codex;
mod restore;
mod apply;
mod live;
mod misc;
