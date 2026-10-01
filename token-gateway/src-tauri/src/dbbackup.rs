//! 資料庫備份管理（P4.8，對標 cc-switch 設定頁的 Backup Management）。
//!
//! ## cc-switch 的實際行為（照它手冊 §1.5 抄）
//!
//! - **自動備份間隔**：停用／6h／12h／24h／48h／7d（**預設 24 小時**）。
//! - **保留份數**：3／5／10／15／20／30／50（**預設 10**）；超過的舊備份自動刪除。
//! - **備份清單**：顯示名稱（自動命名，例如 `db_backup_20260315_143000`）、建立時間、檔案大小。
//! - **操作**：立即備份、還原（**還原前一定先自動備份當前資料庫**）、改名、刪除（需確認）。
//! - 另外還有「匯出／匯入 SQL 備份」，那是另一件事（我們已有來源 JSON 匯出／匯入）。
//!
//! ## 我們怎麼做（兩個關鍵決定）
//!
//! 1. **用 SQLite 的線上備份 API**（`rusqlite` 的 `backup` 功能），不是複製檔案。
//!    應用程式開著時直接 copy `app.db` 可能拿到「複製到一半」的檔案；
//!    線上備份會走 SQLite 自己的頁面鎖，拿到一致快照。
//! 2. **還原是「寫回現有連線」**，不是把檔案換掉。把檔案換掉等於在開啟中的連線底下
//!    抽換資料，接下來的寫入可能就寫進舊檔或直接鎖死；用備份 API 寫回同一個連線
//!    就沒有這個問題（也不必叫使用者重啟）。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

/// 備份檔案命名前綴（與 cc-switch 相同風格）。
pub(crate) const PREFIX: &str = "db_backup_";

/// 自動備份間隔的選項（小時；0＝停用）。
pub(crate) const INTERVAL_OPTIONS: &[i64] = &[0, 6, 12, 24, 48, 168];
/// 保留份數的選項。
pub(crate) const RETENTION_OPTIONS: &[i64] = &[3, 5, 10, 15, 20, 30, 50];
/// 預設值（與 cc-switch 相同）。
pub(crate) const DEFAULT_INTERVAL_HOURS: i64 = 24;
pub(crate) const DEFAULT_RETENTION: i64 = 10;
/// 排程執行緒的檢查間隔（秒）。
const TICK_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct BackupInfo {
    /// 顯示名稱（＝檔名去掉 `.db`）
    pub name: String,
    pub path: String,
    pub created_ms: i64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct RestoreOutcome {
    pub restored: String,
    /// 還原前自動建立的安全備份（cc-switch 也會做）
    pub safety_backup: String,
    pub note: String,
}

/// 備份目錄。
pub fn backup_dir(app_data: &Path) -> PathBuf {
    app_data.join("db-backups")
}

/// 依時間戳產生預設備份名稱（`db_backup_20260315_143000`）。
///
/// 用**本地時間**：備份名稱是給人看的，使用者看到的是自己的牆上時間。
/// （第一版寫成 UTC，實機驗證時名稱顯示 04:29、檔案時間卻是 12:29 —— 差 8 小時。）
pub fn default_name(now_ms: i64) -> String {
    match chrono::TimeZone::timestamp_millis_opt(&chrono::Local, now_ms).single() {
        Some(dt) => format!("{PREFIX}{}", dt.format("%Y%m%d_%H%M%S")),
        None => format!("{PREFIX}{now_ms}"),
    }
}

/// 從檔名回推建立時間（解析不出來就 0）；名稱是本地時間，所以解析時也用本地時區。
pub(crate) fn created_from_name(name: &str) -> i64 {
    let stamp = name.strip_prefix(PREFIX).unwrap_or(name);
    chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d_%H%M%S")
        .ok()
        .and_then(|d| {
            chrono::TimeZone::from_local_datetime(&chrono::Local, &d)
                .single()
                .map(|dt| dt.timestamp_millis())
        })
        .unwrap_or(0)
}

/// 檢查備份名稱是否安全（只用我們自己的命名，不接受路徑分隔）。
pub(crate) fn validate_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 120 {
        return Err("備份名稱長度不合法".to_string());
    }
    let ok = n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !ok || n.contains("..") || n.starts_with('.') {
        return Err(format!("備份名稱只能是英數與 _ - . ：{n}"));
    }
    Ok(n.to_string())
}

/// 依保留份數決定要刪掉哪些（**最舊的**先刪；回傳應刪除的名稱）。
pub(crate) fn prune_plan(mut items: Vec<BackupInfo>, keep: i64) -> Vec<String> {
    let keep = keep.max(1) as usize;
    // 新到舊
    items.sort_by_key(|b| std::cmp::Reverse(b.created_ms));
    items.into_iter().skip(keep).map(|b| b.name).collect()
}

/// 現在該不該自動備份（`last_ms` ＝上次備份時間；0＝從未備份過）。
pub(crate) fn is_due(last_ms: i64, interval_hours: i64, now_ms: i64) -> bool {
    if interval_hours <= 0 {
        return false; // 停用
    }
    if last_ms <= 0 {
        return true; // 從沒備份過 → 立刻做一次
    }
    now_ms - last_ms >= interval_hours * 3_600_000
}

/// 列出備份（新到舊）。
pub fn list(app_data: &Path) -> Vec<BackupInfo> {
    let dir = backup_dir(app_data);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out: Vec<BackupInfo> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("db"))
        .map(|e| {
            let name = e
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let meta = e.metadata().ok();
            // 建立時間優先看**檔案的修改時間**：自己命名的備份（或改過名的）
            // 從名字解析不出時間，只看名字會全部變成 0（排序與保留策略都會亂掉）——
            // 這是 `retention_prunes_on_create` 抓到的。
            let created = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .filter(|v| *v > 0)
                .unwrap_or_else(|| created_from_name(&name));
            BackupInfo {
                name,
                path: e.path().to_string_lossy().to_string(),
                created_ms: created,
                bytes: meta.map(|m| m.len()).unwrap_or(0),
            }
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created_ms));
    out
}

/// 立即備份（線上備份；順便套用保留份數）。
pub fn create(conn: &Connection, app_data: &Path, name: Option<String>) -> Result<BackupInfo, String> {
    let dir = backup_dir(app_data);
    std::fs::create_dir_all(&dir).map_err(|e| format!("建立備份目錄失敗：{e}"))?;
    let now = crate::fsutil::now_ms();
    let base = match name {
        Some(n) => validate_name(&n)?,
        None => default_name(now),
    };
    let path = crate::fsutil::unique_backup_name(&dir, &base).with_extension("db");
    run_backup(conn, &path)?;
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let out = BackupInfo {
        name: path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(base),
        path: path.to_string_lossy().to_string(),
        created_ms: now,
        bytes,
    };
    for n in prune_plan(list(app_data), retention_setting(conn)) {
        let _ = delete(app_data, &n);
    }
    Ok(out)
}

/// 線上備份：把目前連線的內容寫進 `dst`。
fn run_backup(conn: &Connection, dst: &Path) -> Result<(), String> {
    if dst.exists() {
        std::fs::remove_file(dst).map_err(|e| format!("覆蓋舊備份失敗：{e}"))?;
    }
    let mut target =
        Connection::open(dst).map_err(|e| format!("建立備份檔失敗：{e}"))?;
    let backup = rusqlite::backup::Backup::new(conn, &mut target)
        .map_err(|e| format!("啟動備份失敗：{e}"))?;
    backup
        .run_to_completion(64, std::time::Duration::from_millis(5), None)
        .map_err(|e| format!("備份失敗：{e}"))?;
    drop(backup);
    drop(target);
    Ok(())
}

/// 還原：先做安全備份，再用線上備份 API 把內容寫回**現有連線**。
///
/// 注意簽名是 `&mut Connection`：SQLite 的備份 API 需要目的端可變（來源端不用）。
pub fn restore(
    conn: &mut Connection,
    app_data: &Path,
    name: &str,
) -> Result<RestoreOutcome, String> {
    let name = validate_name(name)?;
    let src = backup_dir(app_data).join(format!("{name}.db"));
    if !src.is_file() {
        return Err(format!("找不到備份：{name}"));
    }
    let safety = create(conn, app_data, None)?;
    let from = Connection::open(&src).map_err(|e| format!("開啟備份失敗：{e}"))?;
    {
        let backup = rusqlite::backup::Backup::new(&from, conn)
            .map_err(|e| format!("啟動還原失敗：{e}"))?;
        backup
            .run_to_completion(64, std::time::Duration::from_millis(5), None)
            .map_err(|e| format!("還原失敗：{e}"))?;
    }
    drop(from);
    // 還原後讀一次 schema 版本，確認寫回來的是完整的資料庫
    let version: i32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap_or(0);
    if version <= 0 {
        return Err("還原後讀不到 schema 版本 —— 備份檔可能不完整".to_string());
    }
    Ok(RestoreOutcome {
        restored: name,
        safety_backup: safety.name,
        note: format!("已還原（還原前已自動備份當前資料庫；schema v{version}）"),
    })
}

/// 刪除一個備份。
pub fn delete(app_data: &Path, name: &str) -> Result<(), String> {
    let name = validate_name(name)?;
    let p = backup_dir(app_data).join(format!("{name}.db"));
    if !p.is_file() {
        return Err(format!("找不到備份：{name}"));
    }
    std::fs::remove_file(&p).map_err(|e| format!("刪除失敗：{e}"))
}

/// 改名（只改檔名，內容不動）。
pub fn rename(app_data: &Path, name: &str, new_name: &str) -> Result<BackupInfo, String> {
    let name = validate_name(name)?;
    let new_name = validate_name(new_name)?;
    let dir = backup_dir(app_data);
    let from = dir.join(format!("{name}.db"));
    if !from.is_file() {
        return Err(format!("找不到備份：{name}"));
    }
    let to = dir.join(format!("{new_name}.db"));
    if to.exists() {
        return Err(format!("已經有同名備份：{new_name}"));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("改名失敗：{e}"))?;
    Ok(BackupInfo {
        name: new_name.clone(),
        path: to.to_string_lossy().to_string(),
        created_ms: created_from_name(&new_name),
        bytes: std::fs::metadata(&to).map(|m| m.len()).unwrap_or(0),
    })
}

// ------------------------------------------------------------ 設定 ---

/// 讀自動備份間隔（小時；0＝停用）。
pub fn interval_hours(conn: &Connection) -> i64 {
    crate::db::get_setting(conn, "db_backup_interval")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| INTERVAL_OPTIONS.contains(v))
        .unwrap_or(DEFAULT_INTERVAL_HOURS)
}

/// 讀保留份數。
pub fn retention(conn: &Connection) -> i64 {
    crate::db::get_setting(conn, "db_backup_keep")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| RETENTION_OPTIONS.contains(v))
        .unwrap_or(DEFAULT_RETENTION)
}

fn retention_setting(conn: &Connection) -> i64 {
    retention(conn)
}

/// 上次自動備份時間（沒有任何備份就 0）。
pub fn last_backup_ms(app_data: &Path) -> i64 {
    list(app_data).first().map(|b| b.created_ms).unwrap_or(0)
}

/// 背景排程：每 5 分鐘看一次，該備份就備份（失敗只印訊息，不影響 App）。
pub fn spawn_scheduler(app: tauri::AppHandle) {
    use tauri::Manager;
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(TICK_SECS));
        let Ok(dir) = app.path().app_data_dir() else {
            continue;
        };
        let db = app.state::<crate::db::DbState>();
        let Ok(conn) = db.conn.lock() else { continue };
        let interval = interval_hours(&conn);
        if !is_due(last_backup_ms(&dir), interval, crate::fsutil::now_ms()) {
            continue;
        }
        match create(&conn, &dir, None) {
            Ok(b) => eprintln!("db-backup: 自動備份完成 {}", b.name),
            Err(e) => eprintln!("db-backup: 自動備份失敗 {e}"),
        }
    });
}

/// 給 UI 的狀態。
#[derive(Debug, Clone, Serialize)]
pub struct BackupState {
    pub dir: String,
    pub interval_hours: i64,
    pub retention: i64,
    pub interval_options: Vec<i64>,
    pub retention_options: Vec<i64>,
    pub backups: Vec<BackupInfo>,
    pub last_ms: i64,
}

pub fn state(conn: &Connection, app_data: &Path) -> BackupState {
    BackupState {
        dir: backup_dir(app_data).to_string_lossy().to_string(),
        interval_hours: interval_hours(conn),
        retention: retention(conn),
        interval_options: INTERVAL_OPTIONS.to_vec(),
        retention_options: RETENTION_OPTIONS.to_vec(),
        backups: list(app_data),
        last_ms: last_backup_ms(app_data),
    }
}

#[cfg(test)]
mod tests;
