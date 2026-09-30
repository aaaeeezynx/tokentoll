//! 技能命令（P3.3）。
//!
//! **鎖不跨 `.await`**：`rusqlite::Connection` 是 `Send` 但非 `Sync`，把
//! `MutexGuard` 帶過 `.await` 會讓 future 變成 `!Send`，Tauri 的 async 命令就不成立。
//! 所以每個 async 命令都是「先鎖→讀需要的資料→放掉鎖→await 網路→再鎖→落地」。

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::skills::{
    self, BackupEntry, InstallOutcome, InstalledSkill, RemoteSkill, SkillRepo, SyncReport,
};

fn app_data(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// 儲存庫清單（含內建；第一次呼叫時把內建補進去）。
#[tauri::command]
pub fn skills_repos(db: State<DbState>) -> Result<Vec<SkillRepo>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    skills::store::ensure_builtin_repos(&conn)?;
    skills::list_repos(&conn)
}

#[tauri::command]
pub fn skills_repo_add(
    db: State<DbState>,
    owner: String,
    name: String,
    branch: Option<String>,
    subdir: Option<String>,
    label: Option<String>,
) -> Result<SkillRepo, String> {
    let (o, n, b, d) = skills::validate_repo(
        &owner,
        &name,
        branch.as_deref().unwrap_or("main"),
        subdir.as_deref().unwrap_or(""),
    )?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    skills::add_repo(&conn, &o, &n, &b, &d, label.as_deref().unwrap_or(""), false)
}

#[tauri::command]
pub fn skills_repo_delete(db: State<DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    skills::delete_repo(&conn, id)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoverResult {
    pub skills: Vec<RemoteSkill>,
    /// 掃描失敗的儲存庫（名稱, 原因）—— 單一失敗不影響其他
    pub errors: Vec<(String, String)>,
}

/// 掃描所有儲存庫裡的技能（標上已安裝／可更新）。
#[tauri::command]
pub async fn skills_discover(app: AppHandle) -> Result<DiscoverResult, String> {
    // ① 鎖內：讀儲存庫清單與已安裝清單
    let (repos, installed) = {
        let db = app.state::<DbState>();
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        skills::store::ensure_builtin_repos(&conn)?;
        (skills::list_repos(&conn)?, skills::list_skills(&conn)?)
    };
    // ② 無鎖：打 GitHub
    let (mut remote, errors) = skills::github::list_all(&repos).await;
    // ③ 標記狀態（已安裝 / 來源相同 → 可更新；真正的差異在更新時用雜湊判斷）
    for r in &mut remote {
        if let Some(s) = installed.iter().find(|s| s.name == r.name) {
            r.installed = true;
            r.update_available = s.repo_id == Some(r.repo_id) && s.remote_path == r.path;
        }
    }
    Ok(DiscoverResult {
        skills: remote,
        errors,
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct InstallArgs {
    pub repo_id: i64,
    pub remote_path: String,
    pub name: String,
    /// 要同步到哪些工具（空＝只裝母本，不同步）
    #[serde(default)]
    pub apps: Vec<String>,
}

/// 安裝一個技能。
#[tauri::command]
pub async fn skills_install(
    app: AppHandle,
    db: State<'_, DbState>,
    args: InstallArgs,
) -> Result<InstallOutcome, String> {
    let (repo, env) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        (
            skills::store::get_repo(&conn, args.repo_id)?,
            skills::sync::SkillEnv::real(&conn, &app_data(&app))?,
        )
    };
    let files = skills::github::fetch_skill(&repo, &args.remote_path).await?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    skills::install::install_from_files(
        &conn,
        &env,
        &repo,
        &args.remote_path,
        &args.name,
        &files,
        &args.apps,
    )
}

/// 已安裝技能清單（含母本大小與逐工具綁定）。
#[tauri::command]
pub fn skills_list(app: AppHandle, db: State<DbState>) -> Result<Vec<InstalledSkill>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let env = skills::sync::SkillEnv::real(&conn, &app_data(&app))?;
    skills::install::list_with_sizes(&conn, &env)
}

/// 設定某個技能在某個工具上的啟用（改完立刻同步）。
#[tauri::command]
pub fn skills_set_binding(
    app: AppHandle,
    db: State<DbState>,
    id: i64,
    target: String,
    enabled: bool,
) -> Result<SyncReport, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let env = skills::sync::SkillEnv::real(&conn, &app_data(&app))?;
    skills::set_binding(&conn, id, &target, enabled)?;
    let skill = skills::list_skills(&conn)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("技能不存在（{id}）"))?;
    skills::sync_skill(&skill.name, &env, &skills::sync::enabled_apps(&skill.bindings))
}

/// 更新一個技能（遠端內容雜湊有變才覆寫）。
#[tauri::command]
pub async fn skills_update(
    app: AppHandle,
    db: State<'_, DbState>,
    name: String,
) -> Result<InstallOutcome, String> {
    let (repo, remote_path, env) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let Some(s) = skills::store::get_skill_by_name(&conn, &name)? else {
            return Err(format!("技能「{name}」沒有安裝"));
        };
        let Some(rid) = s.repo_id else {
            return Err(format!("技能「{name}」沒有來源儲存庫，無法更新"));
        };
        (
            skills::store::get_repo(&conn, rid)?,
            s.remote_path,
            skills::sync::SkillEnv::real(&conn, &app_data(&app))?,
        )
    };
    let files = skills::github::fetch_skill(&repo, &remote_path).await?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    skills::install::update_from_files(&conn, &env, &name, &repo, &files)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateOne {
    pub skill: String,
    pub ok: bool,
    pub message: String,
}

/// 全部更新（單一失敗不影響其他）。
#[tauri::command]
pub async fn skills_update_all(
    app: AppHandle,
    db: State<'_, DbState>,
) -> Result<Vec<UpdateOne>, String> {
    let targets: Vec<(String, i64, String)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        skills::list_skills(&conn)?
            .into_iter()
            .filter_map(|s| s.repo_id.map(|r| (s.name, r, s.remote_path)))
            .collect()
    };
    let mut out = vec![];
    for (name, repo_id, path) in targets {
        let loaded = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            skills::store::get_repo(&conn, repo_id).and_then(|r| {
                Ok((r, skills::sync::SkillEnv::real(&conn, &app_data(&app))?))
            })
        };
        let result = match loaded {
            Err(e) => Err(e),
            Ok((repo, env)) => match skills::github::fetch_skill(&repo, &path).await {
                Err(e) => Err(e),
                Ok(files) => {
                    let conn = db.conn.lock().map_err(|e| e.to_string())?;
                    skills::install::update_from_files(&conn, &env, &name, &repo, &files)
                }
            },
        };
        out.push(match result {
            Ok(o) => UpdateOne {
                skill: name,
                ok: true,
                message: if o.unchanged {
                    "已是最新".to_string()
                } else {
                    format!("已更新（{} 個檔案）", o.files)
                },
            },
            Err(e) => UpdateOne {
                skill: name,
                ok: false,
                message: e,
            },
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UninstallResult {
    pub removed: Vec<String>,
    pub backup: String,
}

/// 解除安裝（先備份母本，再從所有工具目錄移除）。
#[tauri::command]
pub fn skills_uninstall(
    app: AppHandle,
    db: State<DbState>,
    name: String,
) -> Result<UninstallResult, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let env = skills::sync::SkillEnv::real(&conn, &app_data(&app))?;
    let (removed, backup) = skills::uninstall_skill(&conn, &env, &name)?;
    Ok(UninstallResult { removed, backup })
}

#[tauri::command]
pub fn skills_backups(app: AppHandle) -> Vec<BackupEntry> {
    skills::list_backups(&app_data(&app))
}

#[tauri::command]
pub fn skills_restore(
    app: AppHandle,
    db: State<DbState>,
    backup: String,
    apps: Vec<String>,
) -> Result<InstallOutcome, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let env = skills::sync::SkillEnv::real(&conn, &app_data(&app))?;
    skills::install::restore_backup(&conn, &env, &backup, &apps)
}

#[tauri::command]
pub fn skills_backup_delete(app: AppHandle, backup: String) -> Result<(), String> {
    skills::install::delete_backup(&app_data(&app), &backup)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillSettings {
    pub storage: String,
    pub sync_mode: String,
}

/// 目前設定（儲存位置、同步方式）。
#[tauri::command]
pub fn skills_settings(db: State<DbState>) -> Result<SkillSettings, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(SkillSettings {
        storage: skills::sync::setting(&conn, "skill_storage", "builtin"),
        sync_mode: skills::sync::setting(&conn, "skill_sync", "symlink"),
    })
}

/// 改設定（儲存位置 builtin／agents、同步方式 symlink／copy）。
#[tauri::command]
pub fn skills_set_settings(
    db: State<DbState>,
    storage: Option<String>,
    sync_mode: Option<String>,
) -> Result<SkillSettings, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    if let Some(s) = storage.as_deref() {
        if s != "builtin" && s != "agents" {
            return Err("儲存位置只能是 builtin 或 agents".to_string());
        }
        crate::db::set_setting(&conn, "skill_storage", s).map_err(|e| e.to_string())?;
    }
    if let Some(m) = sync_mode.as_deref() {
        if m != "symlink" && m != "copy" {
            return Err("同步方式只能是 symlink 或 copy".to_string());
        }
        crate::db::set_setting(&conn, "skill_sync", m).map_err(|e| e.to_string())?;
    }
    Ok(SkillSettings {
        storage: skills::sync::setting(&conn, "skill_storage", "builtin"),
        sync_mode: skills::sync::setting(&conn, "skill_sync", "symlink"),
    })
}
