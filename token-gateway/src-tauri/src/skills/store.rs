//! 技能相關的資料庫存取（儲存庫、已安裝技能、逐工具綁定）。

use rusqlite::{params, Connection};

use super::{InstalledSkill, SkillBinding, SkillRepo};
use crate::fsutil::now_ms;
use crate::tools::TAKEOVER_APPS;

// ------------------------------------------------------------ 儲存庫 ---

fn row_to_repo(r: &rusqlite::Row) -> rusqlite::Result<SkillRepo> {
    Ok(SkillRepo {
        id: r.get(0)?,
        owner: r.get(1)?,
        name: r.get(2)?,
        branch: r.get(3)?,
        subdir: r.get(4)?,
        label: r.get(5)?,
        builtin: r.get::<_, i64>(6)? != 0,
    })
}

const REPO_SELECT: &str =
    "SELECT id, owner, name, branch, subdir, label, builtin FROM skill_repos";

pub fn list_repos(conn: &Connection) -> Result<Vec<SkillRepo>, String> {
    let mut stmt = conn
        .prepare(&format!("{REPO_SELECT} ORDER BY builtin DESC, id ASC"))
        .map_err(|e| e.to_string())?;
    let out = stmt
        .query_map([], row_to_repo)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(out)
}

pub fn get_repo(conn: &Connection, id: i64) -> Result<SkillRepo, String> {
    conn.query_row(&format!("{REPO_SELECT} WHERE id=?1"), [id], row_to_repo)
        .map_err(|e| format!("儲存庫不存在（{id}）：{e}"))
}

/// 新增儲存庫（重複的座標直接回既有那一列 —— 內建與手動都靠 UNIQUE 去重）。
pub fn add_repo(
    conn: &Connection,
    owner: &str,
    name: &str,
    branch: &str,
    subdir: &str,
    label: &str,
    builtin: bool,
) -> Result<SkillRepo, String> {
    conn.execute(
        "INSERT OR IGNORE INTO skill_repos (owner, name, branch, subdir, label, builtin, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            owner,
            name,
            branch,
            subdir,
            label,
            if builtin { 1 } else { 0 },
            now_ms()
        ],
    )
    .map_err(|e| e.to_string())?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM skill_repos WHERE owner=?1 AND name=?2 AND branch=?3 AND subdir=?4",
            params![owner, name, branch, subdir],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    get_repo(conn, id)
}

/// 刪除儲存庫（內建的不給刪）。
pub fn delete_repo(conn: &Connection, id: i64) -> Result<(), String> {
    let r = get_repo(conn, id)?;
    if r.builtin {
        return Err(format!("「{}」是內建儲存庫，不能刪除", r.label));
    }
    conn.execute("DELETE FROM skill_repos WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 確定內建儲存庫都在（啟動時呼叫，缺什麼補什麼）。
pub fn ensure_builtin_repos(conn: &Connection) -> Result<(), String> {
    for (owner, name, branch, subdir, label) in super::BUILTIN_REPOS {
        add_repo(conn, owner, name, branch, subdir, label, true)?;
    }
    Ok(())
}

// ------------------------------------------------------------ 技能 ---

fn row_to_skill(r: &rusqlite::Row) -> rusqlite::Result<InstalledSkill> {
    Ok(InstalledSkill {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        repo_id: r.get(3)?,
        repo_label: r.get(4)?,
        remote_path: r.get(5)?,
        content_hash: r.get(6)?,
        installed_at: r.get(7)?,
        updated_at: r.get(8)?,
        size: 0,
        bindings: Vec::new(),
    })
}

const SKILL_SELECT: &str = "SELECT s.id, s.name, s.description, s.repo_id,
                                   COALESCE(r.label, ''), s.remote_path, s.content_hash,
                                   s.installed_at, s.updated_at
                            FROM skills s LEFT JOIN skill_repos r ON r.id = s.repo_id";

pub fn list_skills(conn: &Connection) -> Result<Vec<InstalledSkill>, String> {
    let mut stmt = conn
        .prepare(&format!("{SKILL_SELECT} ORDER BY s.name ASC"))
        .map_err(|e| e.to_string())?;
    let mut out: Vec<InstalledSkill> = stmt
        .query_map([], row_to_skill)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    let binds = binding_map(conn)?;
    for s in &mut out {
        s.bindings = TAKEOVER_APPS
            .iter()
            .map(|app| SkillBinding {
                app: (*app).to_string(),
                enabled: binds.iter().any(|(sid, a, on)| *sid == s.id && a == app && *on),
            })
            .collect();
    }
    Ok(out)
}

pub(crate) fn get_skill_by_name(
    conn: &Connection,
    name: &str,
) -> Result<Option<InstalledSkill>, String> {
    let mut stmt = conn
        .prepare(&format!("{SKILL_SELECT} WHERE s.name=?1"))
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([name], row_to_skill)
        .map_err(|e| e.to_string())?;
    Ok(rows.next().and_then(|r| r.ok()))
}

pub(crate) fn binding_map(conn: &Connection) -> Result<Vec<(i64, String, bool)>, String> {
    let mut stmt = conn
        .prepare("SELECT skill_id, app, enabled FROM skill_bindings")
        .map_err(|e| e.to_string())?;
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? != 0,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(out)
}

/// 寫入（或更新）一筆技能紀錄，回傳 id。
pub(crate) fn upsert_skill(
    conn: &Connection,
    name: &str,
    description: &str,
    repo_id: Option<i64>,
    remote_path: &str,
    content_hash: &str,
) -> Result<i64, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO skills (name, description, repo_id, remote_path, content_hash,
                             installed_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?6)
         ON CONFLICT(name) DO UPDATE SET
           description=excluded.description, repo_id=excluded.repo_id,
           remote_path=excluded.remote_path, content_hash=excluded.content_hash,
           updated_at=excluded.updated_at",
        params![name, description, repo_id, remote_path, content_hash, now],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row("SELECT id FROM skills WHERE name=?1", [name], |r| r.get(0))
        .map_err(|e| e.to_string())
}

pub(crate) fn delete_skill(conn: &Connection, name: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM skill_bindings WHERE skill_id IN (SELECT id FROM skills WHERE name=?1)",
        [name],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM skills WHERE name=?1", [name])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 設定某個技能在某個工具上的啟用。
pub fn set_binding(
    conn: &Connection,
    skill_id: i64,
    app: &str,
    enabled: bool,
) -> Result<(), String> {
    if !TAKEOVER_APPS.contains(&app) {
        return Err(format!(
            "不支援的工具「{app}」（技能只同步給 {}）",
            TAKEOVER_APPS.join("／")
        ));
    }
    conn.execute(
        "INSERT INTO skill_bindings (skill_id, app, enabled) VALUES (?1,?2,?3)
         ON CONFLICT(skill_id, app) DO UPDATE SET enabled=excluded.enabled",
        params![skill_id, app, if enabled { 1 } else { 0 }],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 這個技能目前記錄的內容雜湊是否等於給定的（更新偵測用）。
#[cfg(test)]
pub fn skill_hash_matches(conn: &Connection, name: &str, hash: &str) -> Result<bool, String> {
    Ok(get_skill_by_name(conn, name)?
        .map(|s| s.content_hash == hash)
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn repo_crud_and_builtin_protection() {
        let (_d, conn) = db();
        ensure_builtin_repos(&conn).unwrap();
        ensure_builtin_repos(&conn).unwrap(); // 冪等
        let repos = list_repos(&conn).unwrap();
        assert_eq!(repos.len(), super::super::BUILTIN_REPOS.len());
        assert!(repos[0].builtin, "內建的排最前");
        assert!(delete_repo(&conn, repos[0].id).unwrap_err().contains("內建"));

        // 空 branch 的正規化在 `validate_repo`（命令層統一做），這裡直接傳 main
        let custom = add_repo(&conn, "me", "my-skills", "main", "skills", "我的", false).unwrap();
        assert_eq!(custom.branch, "main");
        assert_eq!(list_repos(&conn).unwrap().len(), 2);
        // 同座標再加一次 → 不會多一列
        let again = add_repo(&conn, "me", "my-skills", "main", "skills", "重複", false).unwrap();
        assert_eq!(again.id, custom.id);
        assert_eq!(list_repos(&conn).unwrap().len(), 2);
        delete_repo(&conn, custom.id).unwrap();
        assert_eq!(list_repos(&conn).unwrap().len(), 1);
    }

    #[test]
    fn skill_upsert_bindings_and_delete() {
        let (_d, conn) = db();
        let repo = add_repo(&conn, "o", "n", "main", "", "L", false).unwrap();
        let id = upsert_skill(&conn, "demo", "示範技能", Some(repo.id), "skills/demo", "hash1").unwrap();
        let list = list_skills(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].description, "示範技能");
        assert_eq!(list[0].repo_label, "L");
        assert_eq!(list[0].bindings.len(), 3, "三個工具都要有綁定狀態");
        assert!(list[0].bindings.iter().all(|b| !b.enabled), "預設不啟用");

        set_binding(&conn, id, "claude", true).unwrap();
        let l2 = list_skills(&conn).unwrap();
        assert!(l2[0].bindings.iter().any(|b| b.app == "claude" && b.enabled));
        assert!(set_binding(&conn, id, "cursor", true).unwrap_err().contains("不支援"));

        assert!(skill_hash_matches(&conn, "demo", "hash1").unwrap());
        assert!(!skill_hash_matches(&conn, "demo", "other").unwrap());
        assert!(!skill_hash_matches(&conn, "nope", "hash1").unwrap());

        // 更新（UPSERT 不新增列）
        upsert_skill(&conn, "demo", "改了", Some(repo.id), "skills/demo", "hash2").unwrap();
        let l3 = list_skills(&conn).unwrap();
        assert_eq!(l3.len(), 1);
        assert_eq!(l3[0].content_hash, "hash2");
        assert_eq!(l3[0].description, "改了");

        delete_skill(&conn, "demo").unwrap();
        assert!(list_skills(&conn).unwrap().is_empty());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM skill_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "綁定列不能留孤兒");
    }
}
