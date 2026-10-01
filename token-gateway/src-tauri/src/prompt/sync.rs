//! 提示詞與實際檔案的同步：寫檔、讀檔、**智慧回填**、首次啟動匯入。
//!
//! ## 智慧回填（cc-switch 的 smart backfill）
//!
//! 使用者的手改不能被切換預設集吃掉。所以**切換之前**先讀現行檔案：
//!
//! - 有啟用中的預設集，而且檔案內容與它不同 → 代表使用者手改過 →
//!   **先把檔案內容存回那個預設集**（`store::overwrite_content`），再切換。
//! - 沒有啟用中的預設集 → 不回填（cc-switch 也是這樣）。
//! - 回填失敗不影響切換（我們把錯誤放在結果裡回報，不讓切換失敗）。
//!
//! ## 首次啟動匯入
//!
//! 某個工具**一列預設集都沒有**、而它的檔案已經有內容時，自動把內容收成一個
//! 預設集（名稱「現有內容」）並設為啟用 —— 這樣使用者第一次打開這個面板時，
//! 他原本的 `AGENTS.md` 不會在切換後消失。這是 cc-switch「On first launch:
//! Automatically imports existing live file content」的同一件事。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::{store, PromptPreset};
use crate::tools::TAKEOVER_APPS;

/// 提示詞備份保留份數。
pub(crate) const PROMPT_BACKUP_KEEP: usize = 5;

/// 首次匯入時用的預設集名稱。
pub(crate) const IMPORTED_NAME: &str = "現有內容";

/// 某個工具的提示詞檔案路徑（cc-switch 手冊 §3.2 的對照表）。
pub fn path_for(app: &str) -> Result<PathBuf, String> {
    let home = crate::tools::user_home()?;
    match app {
        "claude" => Ok(home.join(".claude").join("CLAUDE.md")),
        "codex" => Ok(home.join(".codex").join("AGENTS.md")),
        "opencode" => Ok(home.join(".config").join("opencode").join("AGENTS.md")),
        _ => Err(format!("提示詞不支援的工具：{app}")),
    }
}

/// **安全鎖**：測試中不可以操作「由主目錄解析出來」的真實檔案。
///
/// 由來（2026-10-01，P3.2 實作時真的發生）：單元測試呼叫了會自己解析路徑的
/// `activate_and_sync`，於是把使用者真實的 `~/.codex/AGENTS.md`（4,139 bytes 的
/// 個人化提示）覆蓋成測試字串。內容後來從 Codex 自己的 session log 撈回來
/// （它會把 AGENTS.md 注入 instructions），但這種事不該有機會再發生 ——
/// 所以所有「會解析主目錄」的入口在測試建置裡一律直接拒絕，
/// 測試請走 `*_at` / `*_ex` 版本（自己傳暫存路徑）。
fn guard_not_real_path(path: &Path) -> Result<(), String> {
    if cfg!(test) {
        return Err(format!(
            "測試中拒絕操作真實路徑（{}）—— 請改用 *_at / *_ex 版本並傳暫存路徑",
            path.display()
        ));
    }
    Ok(())
}

/// 讀目前檔案內容（不存在就空字串）。
pub fn live_content(app: &str) -> Result<String, String> {
    let path = path_for(app)?;
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("讀取 {} 失敗：{e}", path.display()))
}

/// 一次同步（含回填）的結果。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct BackfillOutcome {
    pub app: String,
    pub path: String,
    /// 這次回填到哪個預設集（沒有回填就空）
    pub backfilled_into: String,
    /// 回填時覆蓋掉的內容長度（診斷用）
    pub backfilled_bytes: usize,
    /// 是否真的寫了檔案
    pub wrote_file: bool,
    /// 寫檔前備份的路徑
    pub backup: Option<String>,
    /// 首次啟動自動匯入的預設集名稱（沒有就空）
    pub imported: String,
    /// 非致命訊息（例如回填失敗）
    pub message: String,
}

/// 對某個工具跑一次「回填 → 寫檔」。
///
/// 這是**「重新同步」按鈕**與 cc-switch「切換時」語意的那一條：現行檔案被手改過
/// 就先把內容存回啟用中的預設集（手改優先），再確保檔案與預設集一致。
pub fn sync_app(
    conn: &Connection,
    app_data: &Path,
    app: &str,
) -> Result<BackfillOutcome, String> {
    let path = path_for(app)?;
    guard_not_real_path(&path)?;
    sync_app_ex(conn, app_data, app, &path, true)
}

/// **切換預設集**：先把現行檔案回填進「舊的」啟用預設集，再啟用新的並寫檔。
///
/// 順序很重要（這是實作時被測試逼出來的）：
/// 先 `activate` 再同步的話，回填會把檔案內容存進**新的**那個預設集，
/// 使用者剛切過去的內容立刻被舊檔案覆蓋掉。所以一定是
/// 「讀檔 → 回填舊的 → 切換 → 寫新的」。
pub fn activate_and_sync(
    conn: &Connection,
    app_data: &Path,
    app: &str,
    target_id: i64,
) -> Result<BackfillOutcome, String> {
    let path = path_for(app)?;
    guard_not_real_path(&path)?;
    activate_and_sync_at(conn, app_data, app, &path, target_id)
}

/// 同上，但明講要讀寫哪個檔案 —— 測試用。
pub(crate) fn activate_and_sync_at(
    conn: &Connection,
    app_data: &Path,
    app: &str,
    path: &Path,
    target_id: i64,
) -> Result<BackfillOutcome, String> {
    let mut out = BackfillOutcome {
        app: app.to_string(),
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    // ① 回填給「目前啟用中的」那一個（切換前的保護）。
    //    檔案不存在時不回填 —— 沒有檔案就代表「沒有手改過的內容要救」，
    //    否則會把空的內容回填進舊預設集，把使用者的預設集清空。
    if let Some(prev) = store::active_of(conn, app)? {
        if prev.id != target_id && path.exists() {
            let live = read_live(path);
            if live != prev.content {
                match store::overwrite_content(conn, prev.id, &live) {
                    Ok(()) => {
                        out.backfilled_into = prev.name.clone();
                        out.backfilled_bytes = live.len();
                    }
                    Err(e) => out.message = format!("回填失敗（不影響切換）：{e}"),
                }
            }
        }
    }
    // ② 切換
    store::activate(conn, target_id)?;
    // ③ 寫入新預設集的內容（不再回填 —— 內容就是使用者選的那一份）
    write_active_at(conn, app_data, app, path, &mut out)?;
    Ok(out)
}

/// 把「目前啟用中的內容」寫進檔案，**不做回填**。
///
/// 給「編輯啟用中的預設集之後存檔」用：使用者的編輯就是意圖，
/// 這時候把檔案內容回填進預設集等於把他的編輯吃掉。
pub fn write_active(
    conn: &Connection,
    app_data: &Path,
    app: &str,
) -> Result<BackfillOutcome, String> {
    let path = path_for(app)?;
    guard_not_real_path(&path)?;
    let mut out = BackfillOutcome {
        app: app.to_string(),
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    write_active_at(conn, app_data, app, &path, &mut out)?;
    Ok(out)
}

/// 共用的「把啟用中的內容寫進檔案」：內容一致就不寫，寫之前備份。
fn write_active_at(
    conn: &Connection,
    app_data: &Path,
    app: &str,
    path: &Path,
    out: &mut BackfillOutcome,
) -> Result<(), String> {
    let content = store::active_of(conn, app)?
        .map(|p| p.content)
        .unwrap_or_default();
    if read_live(path) == content {
        return Ok(()); // 內容一致就不寫（避免無謂備份與 mtime 變動）
    }
    let old = if path.exists() {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };
    out.backup = old.as_deref().and_then(|t| {
        crate::fsutil::backup_text(
            app_data,
            &format!("{app}-prompt"),
            path,
            t,
            PROMPT_BACKUP_KEEP,
        )
    });
    crate::fsutil::atomic_write(path, content.as_bytes())
        .map_err(|e| format!("寫入 {} 失敗：{e}", path.display()))?;
    out.wrote_file = true;
    Ok(())
}

/// 同上，但明講要讀寫哪個檔案 —— 測試用（不必碰使用者真正的主目錄）。
pub(crate) fn sync_app_ex(
    conn: &Connection,
    app_data: &Path,
    app: &str,
    path: &Path,
    backfill: bool,
) -> Result<BackfillOutcome, String> {
    let mut out = BackfillOutcome {
        app: app.to_string(),
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };

    // ① 首次啟動匯入：一列都沒有、檔案有內容 → 收成預設集並啟用
    if store::list(conn, app)?.is_empty() {
        let live = read_live(path);
        if !live.trim().is_empty() {
            let saved = store::save(
                conn,
                &super::PromptInput {
                    id: None,
                    app: app.to_string(),
                    name: IMPORTED_NAME.to_string(),
                    content: live,
                },
            )?;
            store::activate(conn, saved.id)?;
            out.imported = saved.name.clone();
        }
    }

    // ② 有啟用中的預設集**而且允許回填、檔案也存在**時才回填
    //    （cc-switch：沒有啟用中的就不回填；存檔路徑也不回填，見 `write_active`）
    let Some(active) = store::active_of(conn, app)? else {
        return Ok(out);
    };
    if backfill && path.exists() {
        let live = read_live(path);
        if live != active.content {
            match store::overwrite_content(conn, active.id, &live) {
                Ok(()) => {
                    out.backfilled_into = active.name.clone();
                    out.backfilled_bytes = live.len();
                }
                Err(e) => out.message = format!("回填失敗（不影響切換）：{e}"),
            }
        }
    }

    // ③ 把啟用中的內容寫進檔案
    write_active_at(conn, app_data, app, path, &mut out)?;
    Ok(out)
}

/// 讀檔（不存在或讀不到就空字串 —— 讀不到不該讓整個同步失敗）。
fn read_live(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// 某個工具的面板狀態：檔案路徑、目前檔案內容、啟用中的預設集。
pub fn panel_state(conn: &Connection, app: &str) -> Result<PanelState, String> {
    let path = path_for(app)?;
    let active = store::active_of(conn, app)?;
    Ok(PanelState {
        app: app.to_string(),
        path: path.to_string_lossy().to_string(),
        file_exists: path.exists(),
        live_bytes: read_live(&path).len(),
        active_id: active.as_ref().map(|p| p.id),
        active_name: active.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
        // 檔案內容與啟用中的預設集不同（＝使用者手改過）
        dirty: match &active {
            Some(a) => read_live(&path) != a.content,
            None => false,
        },
    })
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Default)]
pub struct PanelState {
    pub app: String,
    pub path: String,
    pub file_exists: bool,
    pub live_bytes: usize,
    pub active_id: Option<i64>,
    pub active_name: String,
    /// 檔案內容與啟用中的預設集不同 → UI 可以提示「檔案被手改過，切換時會存回預設集」
    pub dirty: bool,
}

/// 只做首次匯入（面板載入時呼叫；不寫檔）。
pub fn ensure_imported(conn: &Connection, app: &str) -> Result<Option<PromptPreset>, String> {
    let path = path_for(app)?;
    guard_not_real_path(&path)?;
    ensure_imported_at(conn, app, &path)
}

/// 同上，但明講要讀哪個檔案 —— 測試用。
pub(crate) fn ensure_imported_at(
    conn: &Connection,
    app: &str,
    path: &Path,
) -> Result<Option<PromptPreset>, String> {
    if !store::list(conn, app)?.is_empty() {
        return Ok(None);
    }
    let live = read_live(path);
    if live.trim().is_empty() {
        return Ok(None);
    }
    let saved = store::save(
        conn,
        &super::PromptInput {
            id: None,
            app: app.to_string(),
            name: IMPORTED_NAME.to_string(),
            content: live,
        },
    )?;
    store::activate(conn, saved.id)?;
    Ok(Some(saved))
}

/// 這個工具有沒有安裝（檔案或它的目錄存在）—— UI 用來顯示「未安裝」。
pub fn app_installed(app: &str) -> bool {
    match path_for(app) {
        Ok(p) => p.exists() || p.parent().is_some_and(|d| d.exists()),
        Err(_) => false,
    }
}

/// 全部支援的工具（給前端列頁籤）。
pub fn supported_apps() -> Vec<String> {
    TAKEOVER_APPS.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
mod tests;
