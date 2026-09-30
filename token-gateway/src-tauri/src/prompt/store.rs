//! 提示詞預設集的資料庫存取。

use rusqlite::{params, Connection};

use super::{PromptInput, PromptPreset};
use crate::fsutil::now_ms;

fn row_to(r: &rusqlite::Row) -> rusqlite::Result<PromptPreset> {
    Ok(PromptPreset {
        id: r.get(0)?,
        app: r.get(1)?,
        name: r.get(2)?,
        content: r.get(3)?,
        active: r.get::<_, i64>(4)? != 0,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

const SELECT: &str = "SELECT id, app, name, content, active, created_at, updated_at
                      FROM prompt_presets";

/// 某個工具的全部預設集（啟用的排最前，其餘依新增順序）。
pub fn list(conn: &Connection, app: &str) -> Result<Vec<PromptPreset>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT} WHERE app=?1 ORDER BY active DESC, id ASC"
        ))
        .map_err(|e| e.to_string())?;
    let out = stmt
        .query_map([app], row_to)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> Result<PromptPreset, String> {
    conn.query_row(&format!("{SELECT} WHERE id=?1"), [id], row_to)
        .map_err(|e| format!("預設集不存在（{id}）：{e}"))
}

/// 目前啟用中的那一個（沒有就 `None`）。
pub fn active_of(conn: &Connection, app: &str) -> Result<Option<PromptPreset>, String> {
    let mut stmt = conn
        .prepare(&format!("{SELECT} WHERE app=?1 AND active=1 LIMIT 1"))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query_map([app], row_to).map_err(|e| e.to_string())?;
    Ok(rows.next().and_then(|r| r.ok()))
}

/// 新增或更新（以 id 判斷）。
///
/// 更新時**不動 active**：啟用狀態只由 `activate`／`deactivate` 改變，
/// 免得「編輯內容」意外把預設集打開或關掉。
pub fn save(conn: &Connection, input: &PromptInput) -> Result<PromptPreset, String> {
    let now = now_ms();
    let id = match input.id {
        Some(id) => {
            let n = conn
                .execute(
                    "UPDATE prompt_presets SET app=?1, name=?2, content=?3, updated_at=?4
                     WHERE id=?5",
                    params![input.app, input.name, input.content, now, id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(format!("預設集不存在（{id}）"));
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO prompt_presets (app, name, content, active, created_at, updated_at)
                 VALUES (?1,?2,?3,0,?4,?4)",
                params![input.app, input.name, input.content, now],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    get(conn, id)
}

/// 直接覆寫內容（回填用；不動名稱與啟用狀態）。
pub(crate) fn overwrite_content(
    conn: &Connection,
    id: i64,
    content: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE prompt_presets SET content=?1, updated_at=?2 WHERE id=?3",
        params![content, now_ms(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 啟用某一個預設集（**同一個工具只會有一個啟用**），回傳被關掉的那一個。
pub fn activate(conn: &Connection, id: i64) -> Result<Option<PromptPreset>, String> {
    let target = get(conn, id)?;
    let prev = active_of(conn, &target.app)?;
    if prev.as_ref().is_some_and(|p| p.id == id) {
        return Ok(None); // 已經是啟用中的那一個
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE prompt_presets SET active=0 WHERE app=?1 AND active=1",
        [&target.app],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE prompt_presets SET active=1, updated_at=?1 WHERE id=?2",
        params![now_ms(), id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(prev)
}

/// 關掉某個工具目前啟用的預設集（**檔案不動**：內容留在磁碟上）。
pub(crate) fn deactivate(conn: &Connection, app: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE prompt_presets SET active=0, updated_at=?1 WHERE app=?2 AND active=1",
        params![now_ms(), app],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 刪除（**啟用中的不能刪** —— cc-switch 也是這條規矩）。
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    let p = get(conn, id)?;
    if p.active {
        return Err(format!(
            "「{}」是啟用中的預設集，不能刪除 —— 請先啟用別的預設集或把它關掉",
            p.name
        ));
    }
    conn.execute("DELETE FROM prompt_presets WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn input(app: &str, name: &str, content: &str) -> PromptInput {
        PromptInput {
            id: None,
            app: app.into(),
            name: name.into(),
            content: content.into(),
        }
    }

    #[test]
    fn crud_roundtrip() {
        let (_d, conn) = db();
        assert!(list(&conn, "codex").unwrap().is_empty());
        let a = save(&conn, &input("codex", "A", "# A")).unwrap();
        assert!(!a.active, "新增的預設集預設不啟用");
        assert_eq!(a.content, "# A");
        let mut up = input("codex", "A2", "# A2");
        up.id = Some(a.id);
        let b = save(&conn, &up).unwrap();
        assert_eq!(b.id, a.id);
        assert_eq!(b.name, "A2");
        assert_eq!(b.content, "# A2");
        // 刪除
        delete(&conn, a.id).unwrap();
        assert!(list(&conn, "codex").unwrap().is_empty());
        assert!(delete(&conn, a.id).unwrap_err().contains("不存在"));
    }

    #[test]
    fn only_one_active_per_app() {
        let (_d, conn) = db();
        let a = save(&conn, &input("codex", "A", "a")).unwrap();
        let b = save(&conn, &input("codex", "B", "b")).unwrap();
        let c_other = save(&conn, &input("claude", "C", "c")).unwrap();

        let prev = activate(&conn, a.id).unwrap();
        assert!(prev.is_none(), "第一個啟用沒有前一個");
        assert!(get(&conn, a.id).unwrap().active);

        // 啟用 b → a 被關掉
        let prev = activate(&conn, b.id).unwrap();
        assert_eq!(prev.map(|p| p.id), Some(a.id));
        assert!(!get(&conn, a.id).unwrap().active);
        assert!(get(&conn, b.id).unwrap().active);
        // 別的工具不受影響
        activate(&conn, c_other.id).unwrap();
        assert!(get(&conn, c_other.id).unwrap().active);
        assert!(get(&conn, b.id).unwrap().active);

        // 重複啟用同一個：不回報前一個
        assert!(activate(&conn, b.id).unwrap().is_none());
        // 清單把啟用的排最前
        let l = list(&conn, "codex").unwrap();
        assert_eq!(l[0].id, b.id);
    }

    #[test]
    fn active_preset_cannot_be_deleted() {
        let (_d, conn) = db();
        let a = save(&conn, &input("codex", "A", "a")).unwrap();
        activate(&conn, a.id).unwrap();
        let e = delete(&conn, a.id).unwrap_err();
        assert!(e.contains("啟用中"), "{e}");
        // 關掉之後就可以刪
        deactivate(&conn, "codex").unwrap();
        assert!(!get(&conn, a.id).unwrap().active);
        delete(&conn, a.id).unwrap();
    }

    #[test]
    fn save_does_not_change_active_state() {
        let (_d, conn) = db();
        let a = save(&conn, &input("codex", "A", "a")).unwrap();
        activate(&conn, a.id).unwrap();
        let mut up = input("codex", "A", "改了內容");
        up.id = Some(a.id);
        let b = save(&conn, &up).unwrap();
        assert!(b.active, "編輯內容不該把啟用狀態弄掉");
        assert_eq!(b.content, "改了內容");
    }

    #[test]
    fn overwrite_content_only_touches_content() {
        let (_d, conn) = db();
        let a = save(&conn, &input("codex", "A", "舊")).unwrap();
        activate(&conn, a.id).unwrap();
        overwrite_content(&conn, a.id, "新").unwrap();
        let got = get(&conn, a.id).unwrap();
        assert_eq!(got.content, "新");
        assert_eq!(got.name, "A");
        assert!(got.active);
    }
}
