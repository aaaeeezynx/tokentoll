//! 技能的儲存與同步（P3.3）。
//!
//! ## 兩層目錄（cc-switch 的模型）
//!
//! 1. **母本儲存目錄**：技能的真正內容放這裡。預設 `<app_data>/skills/`，
//!    可切換到 `~/.agents/skills`（社群慣例的共用目錄）。
//! 2. **各工具目錄**：Claude `~/.claude/skills/`、Codex `~/.codex/skills/`、
//!    OpenCode `~/.config/opencode/skills/`。同步方式預設 **symlink**，
//!    失敗時退回**複製** —— Windows 上建立目錄連結需要開發者模式或系統管理員，
//!    一般使用者會拿到「作業系統原則不允許」的錯誤，所以退回是常態不是例外
//!    （cc-switch 也是 symlink 失敗就複製）。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::InstalledSkill;
use crate::tools::TAKEOVER_APPS;

/// 同步方式（對應設定鍵 `skill_sync`）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    Symlink,
    Copy,
}

impl SyncMode {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "copy" => SyncMode::Copy,
            _ => SyncMode::Symlink,
        }
    }
}

/// 一次同步的結果。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct SyncReport {
    pub skill: String,
    /// 真的建立了連結或複製的目標
    pub linked: Vec<String>,
    /// 移除的目標
    pub removed: Vec<String>,
    /// 退回複製的目標（symlink 失敗時）
    pub copied_fallback: Vec<String>,
    pub message: String,
}

/// 技能操作的環境：母本儲存目錄、各工具目錄、同步方式。
///
/// **為什麼要這個型別**：`sync_skill` 這類函式如果自己解析真實主目錄，
/// 單元測試就會寫到使用者的 `~/.claude/skills`／`~/.codex/skills`（P3.3 實作時真的
/// 發生了：測試把 `demo` 同步進去的同時，也把真實目錄裡的 `demo` 當成「不該啟用」
/// 而刪掉）。所以真實路徑只在命令層解析一次，測試自己傳暫存目錄進來。
#[derive(Debug, Clone)]
pub struct SkillEnv {
    pub app_data: PathBuf,
    pub storage: PathBuf,
    pub dirs: Vec<(String, PathBuf)>,
    pub mode: SyncMode,
}

impl SkillEnv {
    /// 真實環境（命令層用）。測試建置裡直接拒絕 —— 測試請用 `for_test`。
    pub fn real(conn: &Connection, app_data: &Path) -> Result<Self, String> {
        if cfg!(test) {
            return Err(
                "測試中拒絕解析真實主目錄 —— 請用 SkillEnv::for_test 傳暫存目錄".to_string(),
            );
        }
        let setting = sync_setting(conn, "skill_storage", "builtin");
        let mut dirs = vec![];
        for app in TAKEOVER_APPS {
            dirs.push(((*app).to_string(), app_skills_dir(app)?));
        }
        Ok(Self {
            app_data: app_data.to_path_buf(),
            storage: storage_dir(app_data, &setting),
            dirs,
            mode: SyncMode::parse(&sync_setting(conn, "skill_sync", "symlink")),
        })
    }

    /// 測試用：自己指定母本與各工具目錄。
    #[cfg(test)]
    pub fn for_test(storage: PathBuf, dirs: Vec<(String, PathBuf)>) -> Self {
        let app_data = storage
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            app_data,
            storage,
            dirs,
            mode: SyncMode::Copy, // 測試環境建 symlink 會失敗（權限），直接複製
        }
    }
}

/// 讀 skills 相關的設定鍵（把 `setting` 包一層，避免命令層直接碰 sync 內部）。
pub(crate) fn sync_setting(conn: &Connection, key: &str, default: &str) -> String {
    setting(conn, key, default)
}

/// 母本儲存目錄（設定鍵 `skill_storage`：`builtin` 或 `agents`）。
pub fn storage_dir(app_data: &Path, setting: &str) -> PathBuf {
    match setting.trim() {
        "agents" => crate::tools::user_home()
            .map(|h| h.join(".agents").join("skills"))
            .unwrap_or_else(|_| app_data.join("skills")),
        _ => app_data.join("skills"),
    }
}

/// 某個工具的 skills 目錄（cc-switch 手冊 §3.3 的對照表）。
pub fn app_skills_dir(app: &str) -> Result<PathBuf, String> {
    let home = crate::tools::user_home()?;
    match app {
        "claude" => Ok(home.join(".claude").join("skills")),
        "codex" => Ok(home.join(".codex").join("skills")),
        "opencode" => Ok(home.join(".config").join("opencode").join("skills")),
        _ => Err(format!("技能不支援的工具：{app}")),
    }
}

/// 把一個技能同步到它被啟用的工具目錄（未被啟用的會被移除）。
pub fn sync_skill(
    skill_name: &str,
    env: &SkillEnv,
    enabled_apps: &[String],
) -> Result<SyncReport, String> {
    let src = env.storage.join(skill_name);
    if !src.is_dir() {
        return Err(format!(
            "母本目錄不存在：{}（先安裝或放進去）",
            src.display()
        ));
    }
    let mut rep = SyncReport {
        skill: skill_name.to_string(),
        ..Default::default()
    };
    for (app, dir) in &env.dirs {
        let dst = dir.join(skill_name);
        let want = enabled_apps.iter().any(|a| a == app);
        if want {
            if dst.exists() {
                // 已經同步過：複製模式要重新複製（內容可能更新了）；
                // 連結模式只要指向對的地方就不動。
                let is_link = std::fs::symlink_metadata(&dst)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                if is_link && std::fs::read_link(&dst).ok().as_deref() == Some(src.as_path()) {
                    continue;
                }
                remove_entry(&dst)?;
            }
            std::fs::create_dir_all(dir).map_err(|e| format!("建立 {} 失敗：{e}", dir.display()))?;
            match env.mode {
                SyncMode::Symlink => match symlink_dir(&src, &dst) {
                    Ok(()) => rep.linked.push(dst.to_string_lossy().to_string()),
                    Err(e) => {
                        // 退回複製（Windows 沒開開發者模式時是常態：os error 1314）
                        copy_dir(&src, &dst)?;
                        rep.linked.push(dst.to_string_lossy().to_string());
                        rep.copied_fallback
                            .push(format!("{}（連結失敗：{e}）", dst.display()));
                    }
                },
                SyncMode::Copy => {
                    copy_dir(&src, &dst)?;
                    rep.linked.push(dst.to_string_lossy().to_string());
                }
            }
        } else if dst.exists() {
            remove_entry(&dst)?;
            rep.removed.push(dst.to_string_lossy().to_string());
        }
    }
    Ok(rep)
}

/// 把技能從**所有**工具目錄移除（解除安裝時用）。
pub fn remove_from_all_apps(skill_name: &str, env: &SkillEnv) -> Result<Vec<String>, String> {
    let mut removed = vec![];
    for (_app, dir) in &env.dirs {
        let dst = dir.join(skill_name);
        if dst.exists() {
            remove_entry(&dst)?;
            removed.push(dst.to_string_lossy().to_string());
        }
    }
    Ok(removed)
}

/// 移除檔案／目錄／符號連結（連結只刪連結本身，不要跟著刪到母本！）。
fn remove_entry(p: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || meta.is_file() {
        std::fs::remove_file(p).map_err(|e| format!("刪除 {} 失敗：{e}", p.display()))
    } else {
        std::fs::remove_dir_all(p).map_err(|e| format!("刪除 {} 失敗：{e}", p.display()))
    }
}

fn symlink_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(src, dst)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(src, dst)
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("建立 {} 失敗：{e}", dst.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("複製 {} 失敗：{e}", from.display()))?;
        }
    }
    Ok(())
}

/// 母本目錄的內容雜湊（SHA-256 over 排序後的「相對路徑 + NUL + 內容」）。
///
/// 與 cc-switch 一樣用內容雜湊判斷「遠端有沒有變」：檔案順序不影響結果，
/// 任何一個位元組不同都會得到不同的雜湊。
pub fn dir_hash(dir: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut files = collect_files(dir)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (rel, bytes) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 用一組「記憶體裡的檔案」算雜湊（安裝時要比對遠端內容，還沒落地）。
pub(crate) fn files_hash(files: &[(String, Vec<u8>)]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, bytes) in sorted {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(bytes);
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn collect_files(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = vec![];
    fn walk(base: &Path, cur: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        for entry in std::fs::read_dir(cur).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path();
            if p.is_dir() {
                walk(base, &p, out)?;
            } else {
                let rel = p
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read(&p).map_err(|e| e.to_string())?));
            }
        }
        Ok(())
    }
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// 目錄大小（顯示用）。
pub(crate) fn dir_size(dir: &Path) -> u64 {
    collect_files(dir)
        .map(|f| f.iter().map(|(_, b)| b.len() as u64).sum())
        .unwrap_or(0)
}

/// 從 `SKILL.md` 抓描述：YAML front-matter 的 `description:`，否則第一個非標題行。
pub(crate) fn read_description(dir: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(dir.join("SKILL.md")) else {
        return String::new();
    };
    if let Some(rest) = text.strip_prefix("---") {
        for line in rest.lines() {
            if line.trim() == "---" {
                break;
            }
            if let Some(v) = line.trim().strip_prefix("description:") {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return super::super::usage_query::truncate(v, 300);
                }
            }
        }
    }
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l == "---" {
            continue;
        }
        return super::super::usage_query::truncate(l, 300);
    }
    String::new()
}

/// 列出已安裝技能（含母本大小）。
pub(crate) fn with_sizes(storage: &Path, mut list: Vec<InstalledSkill>) -> Vec<InstalledSkill> {
    for s in &mut list {
        s.size = dir_size(&storage.join(&s.name));
    }
    list
}

/// 便利函式：把 DB 裡的綁定轉成「啟用的工具清單」。
pub(crate) fn enabled_apps(bindings: &[super::SkillBinding]) -> Vec<String> {
    bindings
        .iter()
        .filter(|b| b.enabled)
        .map(|b| b.app.clone())
        .collect()
}

/// 讀設定鍵（skills 相關）。
pub(crate) fn setting(conn: &Connection, key: &str, default: &str) -> String {
    crate::db::get_setting(conn, key).unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dirs_match_the_documented_table() {
        let home = crate::tools::user_home().unwrap();
        assert_eq!(app_skills_dir("claude").unwrap(), home.join(".claude").join("skills"));
        assert_eq!(app_skills_dir("codex").unwrap(), home.join(".codex").join("skills"));
        assert_eq!(
            app_skills_dir("opencode").unwrap(),
            home.join(".config").join("opencode").join("skills")
        );
        assert!(app_skills_dir("gemini").is_err());
    }

    #[test]
    fn storage_dir_switches_by_setting() {
        let app_data = PathBuf::from("C:/data");
        assert_eq!(storage_dir(&app_data, "builtin"), app_data.join("skills"));
        assert_eq!(storage_dir(&app_data, ""), app_data.join("skills"));
        let agents = storage_dir(&app_data, "agents");
        assert!(agents.ends_with(Path::new(".agents").join("skills")), "{agents:?}");
    }

    #[test]
    fn hash_is_order_insensitive_and_content_sensitive() {
        let a = vec![
            ("b.txt".to_string(), b"2".to_vec()),
            ("a.txt".to_string(), b"1".to_vec()),
        ];
        let b = vec![
            ("a.txt".to_string(), b"1".to_vec()),
            ("b.txt".to_string(), b"2".to_vec()),
        ];
        assert_eq!(files_hash(&a), files_hash(&b), "順序不影響");
        let c = vec![
            ("a.txt".to_string(), b"1".to_vec()),
            ("b.txt".to_string(), b"3".to_vec()),
        ];
        assert_ne!(files_hash(&a), files_hash(&c), "內容不同就要不同");
        // 路徑也算內容的一部分
        let d = vec![("a.txt".to_string(), b"1".to_vec())];
        let e = vec![("x.txt".to_string(), b"1".to_vec())];
        assert_ne!(files_hash(&d), files_hash(&e));
    }

    #[test]
    fn description_comes_from_front_matter_or_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("SKILL.md");
        std::fs::write(&p, "---\nname: demo\ndescription: 這是一段說明\n---\n\n# 標題\n").unwrap();
        assert_eq!(read_description(dir.path()), "這是一段說明");
        std::fs::write(&p, "# 只有標題\n第二行說明\n").unwrap();
        assert_eq!(read_description(dir.path()), "第二行說明");
        std::fs::write(&p, "# 只有標題\n").unwrap();
        assert_eq!(read_description(dir.path()), "", "只有標題時沒有描述");
        std::fs::remove_file(&p).unwrap();
        assert_eq!(read_description(dir.path()), "");
    }

    #[test]
    fn copy_and_remove_roundtrip_on_temp_dirs() {
        let src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("sub")).unwrap();
        std::fs::write(src.path().join("SKILL.md"), "# x").unwrap();
        std::fs::write(src.path().join("sub").join("a.py"), "print(1)").unwrap();
        assert!(dir_size(src.path()) > 0);
        assert_eq!(dir_hash(src.path()).unwrap().len(), 64, "sha256 hex");

        let dst = tempfile::tempdir().unwrap();
        let target = dst.path().join("copy");
        copy_dir(src.path(), &target).unwrap();
        assert_eq!(dir_hash(src.path()).unwrap(), dir_hash(&target).unwrap());
        remove_entry(&target).unwrap();
        assert!(!target.exists());
    }
}
