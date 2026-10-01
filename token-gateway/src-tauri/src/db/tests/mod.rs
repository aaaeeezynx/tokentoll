// ─────────────────────────────────────────────────────── 單測 ───
//
// 本檔是資料真相層：schema 建錯或 migration 漏跑等於使用者資料損毀。
// 在 Phase 1 之前這裡是 0 測試（見 docs/REFACTORING-PLAN.md §3 B8）。
// 由 `db.rs` 的 `mod tests` 拆分而來：測試內容與斷言一字未改，只依主題拆成
// 子模組；共用 helper 留在本檔，子模組以 `use super::*;` 取用。

use super::*;
use std::path::Path;

    fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
        conn.prepare(&format!(
            "SELECT 1 FROM pragma_table_info('{table}') WHERE name='{col}'"
        ))
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false)
    }

    fn has_table(conn: &Connection, table: &str) -> bool {
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
            .and_then(|mut s| s.exists([table]))
            .unwrap_or(false)
    }

    fn max_version(conn: &Connection) -> i64 {
        conn.query_row("SELECT COALESCE(MAX(version),0) FROM schema_version", [], |r| {
            r.get(0)
        })
        .unwrap_or(0)
    }

    /// `settings` 裡有沒有這個 key（v15 清孤兒列的測試要用）。
    fn has_setting(conn: &Connection, key: &str) -> bool {
        conn.query_row("SELECT 1 FROM settings WHERE key=?1", [key], |r| r
            .get::<_, i64>(0))
            .is_ok()
    }

    fn open(path: &Path) -> Connection {
        open_and_ensure(path).expect("open_and_ensure")
    }

    fn count_rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(-1)
    }

mod fresh;
mod legacy;
mod migrate;
mod removal;
mod seed;
