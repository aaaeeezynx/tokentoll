//! 來源的複製與匯入／匯出（對標 CC Switch 的 duplicate／import-export）。
//!
//! ## 複製
//!
//! 複製的是「來源的全部內容」：連線資訊、**已登記的模型**（含 context_window 與
//! reasoning）、**每模型價格**（`provider_pricing`）、**訂閱時段**（`pricing_periods`）。
//! 刻意**不複製**的三類：
//!
//! - `local_keys`：金鑰綁哪個來源是使用者的明確選擇，複製會多出沒人知道的鑰匙。
//! - `provider_stripped_fields` / `provider_model_protocol`：那是**學到的行為**，
//!   而且協議記憶的鍵含 `declared_format` —— 複製品若改了協議，舊學習本來就不適用。
//! - `model_catalogs`：只是上游 `/models` 的快取，下次抓就好。
//!
//! 複製品插在**原本那筆的後面**（priority 後移一格），不是丟到列表尾端：
//! 複製的用途就是「拿同一組設定改一個欄位試試看」，兩筆相鄰才好比較。
//!
//! ## 匯出／匯入
//!
//! 預設**不含金鑰**（`includes_keys=false`）。含金鑰要使用者明確要求，
//! 而且匯出檔開頭就寫明 `includes_keys: true`，匯入時也會回報 —— 不讓
//! 「檔案裡有明碼金鑰」變成一件靜默的事。
//!
//! 匯入採 **skip 語意**：同一個 `(name, base_url)` 已存在就跳過（回報 `skipped`），
//! 不覆蓋既有設定 —— 匯入不該是最容易弄壞現有東西的操作。名稱若要新增會自動加序號。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::fsutil::now_ms;

/// 匯出格式版本。欄位有破壞性變更時 +1，匯入端據此給出可理解的錯誤。
pub const EXPORT_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportModel {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub actual_model: String,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub ord: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPricing {
    pub model_norm: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub in_pm: f64,
    #[serde(default)]
    pub out_pm: f64,
    #[serde(default)]
    pub cache_read_pm: f64,
    #[serde(default)]
    pub cache_create_pm: f64,
    #[serde(default)]
    pub sub_fee_usd: f64,
    #[serde(default)]
    pub sub_included_tokens: i64,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPeriod {
    #[serde(default = "star")]
    pub model_norm: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default)]
    pub ord: i64,
    #[serde(default)]
    pub in_pm: f64,
    #[serde(default)]
    pub out_pm: f64,
    #[serde(default)]
    pub cache_read_pm: f64,
    #[serde(default)]
    pub cache_create_pm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProvider {
    pub name: String,
    #[serde(default = "universal")]
    pub app_type: String,
    #[serde(default = "chat")]
    pub api_format: String,
    pub base_url: String,
    #[serde(default = "bearer")]
    pub auth_scheme: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models_json: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub models: Vec<ExportModel>,
    #[serde(default)]
    pub pricing: Vec<ExportPricing>,
    #[serde(default)]
    pub periods: Vec<ExportPeriod>,
}

/// 匯出檔的類型標記：匯入時據此擋掉「丟錯檔案」。
///
/// 2026-10-04 品牌改名（Token Gateway → Token Toll），值由
/// `token-gateway/providers` 改為 `tokentoll/providers`；**匯入端新舊都認**
/// （見 [`EXPORT_KINDS`]），使用者先前匯出的檔案仍可匯入。
pub const EXPORT_KIND: &str = "tokentoll/providers";

/// 改名前的類型標記。
pub const EXPORT_KIND_LEGACY: &str = "token-gateway/providers";

/// 匯入時接受的類型標記（新名優先，舊名相容）。
pub const EXPORT_KINDS: [&str; 2] = [EXPORT_KIND, EXPORT_KIND_LEGACY];

/// 匯出檔的檔名前綴：`{stem}-{stamp}.json`。
pub const EXPORT_FILE_STEM: &str = "tokentoll-providers";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFile {
    /// 固定為 [`EXPORT_KIND`]：匯入時據此擋掉「丟錯檔案」。
    pub kind: String,
    pub version: i64,
    #[serde(default)]
    pub exported_at: i64,
    #[serde(default)]
    pub includes_keys: bool,
    pub providers: Vec<ExportProvider>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub added: i64,
    pub skipped: i64,
    pub models: i64,
    pub pricing: i64,
    pub periods: i64,
    /// 實際新增的來源名稱（前端顯示用）
    pub names: Vec<String>,
    /// 匯入檔是否帶金鑰（前端要提醒使用者）
    pub includes_keys: bool,
}

fn yes() -> bool {
    true
}
fn star() -> String {
    "*".to_string()
}
fn universal() -> String {
    "universal".to_string()
}
fn chat() -> String {
    "openai-chat".to_string()
}
fn bearer() -> String {
    "bearer".to_string()
}

fn norm_scheme(s: &str) -> String {
    match s.trim() {
        "goog-key" | "anthropic" => s.trim().to_string(),
        _ => "bearer".to_string(),
    }
}

fn next_priority(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(priority), -1) + 1 FROM providers",
        [],
        |r| r.get(0),
    )
}

/// 名稱去重：`name`、`name 2`、`name 3`…（`suffix` 給匯入用）。
fn unique_name(conn: &Connection, base: &str, suffix: &str) -> rusqlite::Result<String> {
    let stem = if suffix.is_empty() {
        base.to_string()
    } else {
        format!("{base}{suffix}")
    };
    let exists = |n: &str| -> rusqlite::Result<bool> {
        conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE name=?1",
            [n],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
    };
    if !exists(&stem)? {
        return Ok(stem);
    }
    for i in 2..1000 {
        let cand = format!("{stem} {i}");
        if !exists(&cand)? {
            return Ok(cand);
        }
    }
    Ok(format!("{stem} {}", now_ms()))
}

mod duplicate;
mod export;
mod import;

pub(crate) use duplicate::duplicate_provider;
pub(crate) use export::export_providers;
pub(crate) use import::import_providers;

#[tauri::command]
pub fn provider_duplicate(db: State<DbState>, id: i64) -> Result<i64, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    duplicate_provider(&conn, id)
}

/// 匯出成 JSON 字串（前端「複製 JSON」用）。預設不含金鑰。
#[tauri::command]
pub fn providers_export(db: State<DbState>, include_keys: bool) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let file = export_providers(&conn, include_keys)?;
    serde_json::to_string_pretty(&file).map_err(|e| e.to_string())
}

/// 匯出到資料夾裡的檔案，回傳完整路徑。
///
/// **為什麼由 Rust 寫檔而不是靠瀏覽器下載**：2026-10-01 實機驗證發現，第一次
/// 匯出 WebView2 會直接存檔，但**第二次**會跳原生的「想要下載多個檔案」權限
/// 對話框，那個對話框不在 DOM 裡（CDP 也點不到）而且會蓋住整個 UI —— 使用者
/// 會覺得 App 壞了。改成後端寫檔：路徑明確、可連續匯出，而且「匯入」可以用
/// 同一條路徑直接讀回來（見 `providers_import_file`）。
pub(crate) fn write_export(
    dir: &Path,
    stamp: &str,
    json: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("建立目錄失敗（{}）：{e}", dir.display()))?;
    let mut path = dir.join(format!("{EXPORT_FILE_STEM}-{stamp}.json"));
    // 同一秒內連續匯出也不覆蓋前一個
    for i in 2..1000 {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{EXPORT_FILE_STEM}-{stamp}-{i}.json"));
    }
    std::fs::write(&path, json).map_err(|e| format!("寫入失敗（{}）：{e}", path.display()))?;
    Ok(path)
}

/// 匯出目錄：先試系統下載資料夾，沒有就用 App 資料目錄（回傳值會顯示給使用者）。
fn export_dir(app: &AppHandle) -> PathBuf {
    if let Some(d) = dirs::download_dir() {
        return d;
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

#[tauri::command]
pub fn providers_export_file(
    app: AppHandle,
    db: State<DbState>,
    include_keys: bool,
) -> Result<String, String> {
    let json = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let file = export_providers(&conn, include_keys)?;
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?
    };
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let path = write_export(&export_dir(&app), &stamp, &json)?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn providers_import(db: State<DbState>, json: String) -> Result<ImportReport, String> {
    let file: ExportFile =
        serde_json::from_str(&json).map_err(|e| format!("解析匯入檔失敗：{e}"))?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    import_providers(&conn, &file)
}

/// 單一匯入檔的大小上限（避免誤指到巨大檔案把記憶體吃光）。
const IMPORT_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// 從檔案路徑匯入（搭配 `providers_export_file` 給的路徑）。
#[tauri::command]
pub fn providers_import_file(db: State<DbState>, path: String) -> Result<ImportReport, String> {
    let p = PathBuf::from(path.trim());
    let meta = std::fs::metadata(&p).map_err(|e| format!("讀不到檔案（{}）：{e}", p.display()))?;
    if meta.len() > IMPORT_MAX_BYTES {
        return Err(format!(
            "檔案太大（{} bytes > {IMPORT_MAX_BYTES}）：這不太像是來源匯出檔",
            meta.len()
        ));
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("讀取失敗：{e}"))?;
    let file: ExportFile =
        serde_json::from_str(&text).map_err(|e| format!("解析匯入檔失敗：{e}"))?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    import_providers(&conn, &file)
}

#[cfg(test)]
mod tests;
