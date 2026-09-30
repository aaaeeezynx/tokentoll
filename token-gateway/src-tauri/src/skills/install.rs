//! 技能的安裝、更新、解除安裝（P3.3）。
//!
//! ## 流程（與 cc-switch 一致）
//!
//! **安裝**：下載來源技能資料夾 → 寫進母本儲存目錄 → 計算內容雜湊 → 記進資料庫
//! → 依使用者選的工具同步出去（symlink 優先、失敗複製）。
//!
//! **更新**：重新下載 → 比對內容雜湊 → 有變才覆寫母本並重新同步
//! （cc-switch 也是用 SHA-256 比對，不是看時間戳）。
//!
//! **解除安裝**：先把母本**備份**到 `skill-backups/`，再從所有工具目錄移除、
//! 刪母本、刪資料庫那一列。備份可以還原，也可以單獨刪除。
//!
//! ## 為什麼網路與資料庫要分開
//!
//! `rusqlite::Connection` 是 `Send` 但**不是** `Sync`，`MutexGuard` 跨 `.await`
//! 會讓整個 future 變成 `!Send`，Tauri 的 async 命令就不成立（編譯器會說
//! 「future cannot be sent between threads safely」）。所以本模組**全部是同步函式**，
//! 只負責「已經抓好檔案之後要怎麼落地」；真正的下載在命令層、
//! 鎖**先放掉再 await**（見 `commands/skills_cmd.rs`）。

use std::path::Path;

use rusqlite::Connection;

use super::{store, sync, InstalledSkill, SkillRepo};
use sync::SkillEnv;

/// 一次安裝／更新的結果。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct InstallOutcome {
    pub skill: String,
    pub description: String,
    pub files: usize,
    pub hash: String,
    pub storage: String,
    /// 這次是更新（還是首次安裝）
    pub updated: bool,
    /// 內容沒變（更新時可能發生）
    pub unchanged: bool,
    pub sync: sync::SyncReport,
}

/// 備份清單的一列。
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct BackupEntry {
    pub name: String,
    pub skill: String,
    pub at: String,
    pub bytes: u64,
}

/// 把一組檔案寫進 `dst`（先清掉舊的再寫，避免舊檔殘留）。
fn write_files(dst: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    if dst.exists() {
        std::fs::remove_dir_all(dst).map_err(|e| format!("清掉舊的 {} 失敗：{e}", dst.display()))?;
    }
    for (rel, data) in files {
        // 防禦：來源路徑不可以跳出去（tarball 是外部輸入）
        if rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
            return Err(format!("來源含不安全的路径：{rel}"));
        }
        let p = dst.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, data).map_err(|e| format!("寫入 {} 失敗：{e}", p.display()))?;
    }
    Ok(())
}

/// 同步某個技能到它被啟用的工具（讀資料庫裡的綁定）。
fn sync_now(conn: &Connection, env: &SkillEnv, id: i64, name: &str) -> Result<sync::SyncReport, String> {
    let bindings = store::list_skills(conn)?
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.bindings)
        .unwrap_or_default();
    sync::sync_skill(name, env, &sync::enabled_apps(&bindings))
}

/// **已經抓好檔案之後**的落地：寫母本 → 記資料庫 → 同步。
pub fn install_from_files(
    conn: &Connection,
    env: &SkillEnv,
    repo: &SkillRepo,
    remote_path: &str,
    name: &str,
    files: &[(String, Vec<u8>)],
    apps: &[String],
) -> Result<InstallOutcome, String> {
    let name = super::validate_skill_name(name)?;
    if !files.iter().any(|(p, _)| p == "SKILL.md") {
        return Err("來源裡沒有 SKILL.md —— 這不是技能".to_string());
    }
    let hash = sync::files_hash(files);
    let dst = env.storage.join(&name);
    std::fs::create_dir_all(&env.storage)
        .map_err(|e| format!("建立 {} 失敗：{e}", env.storage.display()))?;
    write_files(&dst, files)?;
    let description = sync::read_description(&dst);
    let id = store::upsert_skill(conn, &name, &description, Some(repo.id), remote_path, &hash)?;
    for app in apps {
        store::set_binding(conn, id, app, true)?;
    }
    let rep = sync_now(conn, env, id, &name)?;
    Ok(InstallOutcome {
        skill: name,
        description,
        files: files.len(),
        hash,
        storage: dst.to_string_lossy().to_string(),
        updated: false,
        unchanged: false,
        sync: rep,
    })
}

/// **已經抓好遠端檔案之後**的更新：雜湊沒變就只重新同步。
pub fn update_from_files(
    conn: &Connection,
    env: &SkillEnv,
    name: &str,
    repo: &SkillRepo,
    files: &[(String, Vec<u8>)],
) -> Result<InstallOutcome, String> {
    let Some(skill) = store::get_skill_by_name(conn, name)? else {
        return Err(format!("技能「{name}」沒有安裝"));
    };
    let hash = sync::files_hash(files);
    let dst = env.storage.join(name);
    if hash == skill.content_hash && dst.is_dir() {
        let rep = sync_now(conn, env, skill.id, name)?;
        return Ok(InstallOutcome {
            skill: name.to_string(),
            description: skill.description,
            files: files.len(),
            hash,
            storage: dst.to_string_lossy().to_string(),
            updated: true,
            unchanged: true,
            sync: rep,
        });
    }
    write_files(&dst, files)?;
    let description = sync::read_description(&dst);
    store::upsert_skill(
        conn,
        name,
        &description,
        Some(repo.id),
        &skill.remote_path,
        &hash,
    )?;
    let rep = sync_now(conn, env, skill.id, name)?;
    Ok(InstallOutcome {
        skill: name.to_string(),
        description,
        files: files.len(),
        hash,
        storage: dst.to_string_lossy().to_string(),
        updated: true,
        unchanged: false,
        sync: rep,
    })
}

/// 解除安裝：先備份母本，再從所有工具目錄移除、刪母本與資料庫那一列。
pub fn uninstall_skill(
    conn: &Connection,
    env: &SkillEnv,
    name: &str,
) -> Result<(Vec<String>, String), String> {
    let src = env.storage.join(name);
    let backup = if src.is_dir() {
        let dir = env.app_data.join("skill-backups");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = crate::fsutil::unique_backup_name(
            &dir,
            &format!("{name}-{}", crate::fsutil::backup_stamp()),
        );
        copy_tree(&src, &dest)?;
        Some(dest.to_string_lossy().to_string())
    } else {
        None
    };
    let removed = sync::remove_from_all_apps(name, env)?;
    if src.exists() {
        std::fs::remove_dir_all(&src).map_err(|e| format!("刪除母本失敗：{e}"))?;
    }
    store::delete_skill(conn, name)?;
    Ok((
        removed,
        backup.unwrap_or_else(|| "（母本不存在，沒有備份）".to_string()),
    ))
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

/// 備份清單（目錄名 `{技能}-{YYYYMMDD}-{HHMMSS}`）。
pub fn list_backups(app_data: &Path) -> Vec<BackupEntry> {
    let dir = app_data.join("skill-backups");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out: Vec<BackupEntry> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // 名稱尾端是 `-YYYYMMDD-HHMMSS`，把時間戳與技能名分開
            let at = name
                .rsplit_once('-')
                .and_then(|(head, secs)| {
                    head.rsplit_once('-')
                        .map(|(skill, date)| (skill.to_string(), format!("{date}-{secs}")))
                })
                .unwrap_or_else(|| (name.clone(), String::new()));
            BackupEntry {
                name,
                skill: at.0,
                at: at.1,
                bytes: sync::dir_size(&e.path()),
            }
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

/// 從備份還原（還原後會重新同步到指定的工具）。
pub fn restore_backup(
    conn: &Connection,
    env: &SkillEnv,
    backup_name: &str,
    apps: &[String],
) -> Result<InstallOutcome, String> {
    let src = env.app_data.join("skill-backups").join(backup_name);
    if !src.is_dir() {
        return Err(format!("找不到備份：{backup_name}"));
    }
    let entry = list_backups(&env.app_data)
        .into_iter()
        .find(|b| b.name == backup_name)
        .ok_or_else(|| format!("找不到備份：{backup_name}"))?;
    let name = super::validate_skill_name(&entry.skill)?;
    let dst = env.storage.join(&name);
    std::fs::create_dir_all(&env.storage).map_err(|e| e.to_string())?;
    if dst.exists() {
        std::fs::remove_dir_all(&dst).map_err(|e| e.to_string())?;
    }
    copy_tree(&src, &dst)?;
    let hash = sync::dir_hash(&dst)?;
    let description = sync::read_description(&dst);
    let id = store::upsert_skill(conn, &name, &description, None, "", &hash)?;
    for app in apps {
        store::set_binding(conn, id, app, true)?;
    }
    let rep = sync_now(conn, env, id, &name)?;
    Ok(InstallOutcome {
        skill: name,
        description,
        files: 0,
        hash,
        storage: dst.to_string_lossy().to_string(),
        updated: false,
        unchanged: false,
        sync: rep,
    })
}

/// 刪除一個備份。
pub fn delete_backup(app_data: &Path, backup_name: &str) -> Result<(), String> {
    let p = app_data.join("skill-backups").join(backup_name);
    if !p.is_dir() {
        return Err(format!("找不到備份：{backup_name}"));
    }
    std::fs::remove_dir_all(&p).map_err(|e| e.to_string())
}

/// 給 UI 的技能清單（含母本大小）。
pub fn list_with_sizes(conn: &Connection, env: &SkillEnv) -> Result<Vec<InstalledSkill>, String> {
    Ok(sync::with_sizes(&env.storage, store::list_skills(conn)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    /// 測試環境：母本與三個工具目錄都在暫存區裡。
    fn env(base: &Path) -> SkillEnv {
        let dirs = ["claude", "codex", "opencode"]
            .iter()
            .map(|a| ((*a).to_string(), base.join(a)))
            .collect();
        SkillEnv::for_test(base.join("storage"), dirs)
    }

    fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "SKILL.md".to_string(),
                "---\ndescription: 示範\n---\n".as_bytes().to_vec(),
            ),
            ("python/run.py".to_string(), b"print(1)\n".to_vec()),
        ]
    }

    fn repo(conn: &Connection) -> SkillRepo {
        store::add_repo(conn, "me", "skills", "main", "skills", "我的", false).unwrap()
    }

    /// 安裝：寫母本、記資料庫、依指定工具同步（其他工具不會被碰）。
    #[test]
    fn install_writes_storage_and_syncs_to_selected_apps() {
        let (_d, conn) = db();
        let base = tempfile::tempdir().unwrap();
        let e = env(base.path());
        let r = repo(&conn);
        let out = install_from_files(
            &conn,
            &e,
            &r,
            "skills/demo",
            "demo",
            &files(),
            &["codex".to_string()],
        )
        .unwrap();
        assert_eq!(out.skill, "demo");
        assert_eq!(out.files, 2);
        assert_eq!(out.description, "示範", "描述要從 front-matter 抓");
        assert!(Path::new(&out.storage).join("SKILL.md").is_file());
        assert!(Path::new(&out.storage).join("python").join("run.py").is_file());
        // 只同步到 codex
        assert_eq!(out.sync.linked.len(), 1, "{:?}", out.sync);
        assert!(base.path().join("codex").join("demo").is_dir());
        assert!(!base.path().join("claude").join("demo").exists(), "沒選的工具不該有");
        assert!(!base.path().join("opencode").join("demo").exists());
        let list = store::list_skills(&conn).unwrap();
        let b = &list[0].bindings;
        assert!(b.iter().any(|x| x.app == "codex" && x.enabled));
        assert!(b.iter().all(|x| x.app == "codex" || !x.enabled));

        // 沒有 SKILL.md 的來源要拒絕
        let bad = install_from_files(
            &conn,
            &e,
            &r,
            "skills/x",
            "x",
            &[("a.txt".to_string(), b"1".to_vec())],
            &[],
        );
        assert!(bad.unwrap_err().contains("SKILL.md"));
    }

    /// 更新：內容相同時 `unchanged`，不同時覆寫並換雜湊。
    #[test]
    fn update_detects_changes_by_hash() {
        let (_d, conn) = db();
        let base = tempfile::tempdir().unwrap();
        let e = env(base.path());
        let r = repo(&conn);
        install_from_files(&conn, &e, &r, "skills/demo", "demo", &files(), &[]).unwrap();
        let same = update_from_files(&conn, &e, "demo", &r, &files()).unwrap();
        assert!(same.unchanged, "內容一樣不該重寫");

        let mut changed = files();
        changed[1].1 = b"print(2)\n".to_vec();
        let up = update_from_files(&conn, &e, "demo", &r, &changed).unwrap();
        assert!(up.updated && !up.unchanged);
        assert_ne!(up.hash, same.hash);
        let p = Path::new(&up.storage).join("python").join("run.py");
        assert_eq!(std::fs::read_to_string(p).unwrap(), "print(2)\n");
        // 沒安裝的技能要給明確錯誤
        assert!(
            update_from_files(&conn, &e, "nope", &r, &files())
                .unwrap_err()
                .contains("沒有安裝")
        );
    }

    /// 解除安裝：備份母本、清掉紀錄，備份可以列出、還原、刪除。
    #[test]
    fn uninstall_backs_up_and_can_restore() {
        let (_d, conn) = db();
        let base = tempfile::tempdir().unwrap();
        let e = env(base.path());
        let r = repo(&conn);
        install_from_files(
            &conn,
            &e,
            &r,
            "skills/demo",
            "demo",
            &files(),
            &["claude".to_string()],
        )
        .unwrap();
        assert!(base.path().join("claude").join("demo").is_dir());

        let (removed, backup) = uninstall_skill(&conn, &e, "demo").unwrap();
        assert!(backup.contains("skill-backups"), "{backup}");
        assert!(Path::new(&backup).join("SKILL.md").is_file(), "備份要有內容");
        assert!(store::list_skills(&conn).unwrap().is_empty(), "紀錄要清掉");
        assert!(!base.path().join("claude").join("demo").exists(), "工具目錄要清掉");
        assert_eq!(removed.len(), 1, "{removed:?}");

        let backups = list_backups(&e.app_data);
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].skill, "demo");
        assert!(backups[0].bytes > 0);
        assert!(!backups[0].at.is_empty(), "時間戳要解析出來");

        let out = restore_backup(&conn, &e, &backups[0].name, &["opencode".to_string()]).unwrap();
        assert_eq!(out.skill, "demo");
        assert!(store::list_skills(&conn).unwrap()[0]
            .bindings
            .iter()
            .any(|b| b.app == "opencode" && b.enabled));
        assert!(base.path().join("opencode").join("demo").is_dir());

        delete_backup(&e.app_data, &backups[0].name).unwrap();
        assert!(list_backups(&e.app_data).is_empty());
        assert!(delete_backup(&e.app_data, "nope").unwrap_err().contains("找不到"));
    }

    /// 真實環境在測試建置裡要被拒絕（P3.3 曾因此寫到使用者的 skills 目錄）。
    #[test]
    fn real_env_is_refused_under_test() {
        let (_d, conn) = db();
        let e = SkillEnv::real(&conn, Path::new("C:/tmp"));
        assert!(e.is_err(), "測試中不該能解析真實主目錄");
        assert!(e.unwrap_err().contains("拒絕解析真實主目錄"));
    }
}
