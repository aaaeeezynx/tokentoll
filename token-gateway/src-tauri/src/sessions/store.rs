//! 刪除會話（P3.4）。
//!
//! **安全界線**（這一支會真的刪檔，所以規則寫死）：
//! 1. 只接受 `.jsonl` 檔（Claude／Codex 的會話格式）。
//! 2. 路徑必須在**該工具的會話目錄底下**（`canonicalize` 之後比對字首，
//!    擋掉 `..` 與符號連結的繞路）。
//! 3. OpenCode 是別人的 SQLite → 一律拒絕。
//! 4. 允許的目錄由 [`SessionEnv`] 明講；`SessionEnv::real()` 在**測試建置直接拒絕**
//!    （P3.2／P3.3 兩次「測試寫到真實檔案」的教訓）。
//!
//! 刪掉檔案**不會**撤銷已回填的用量統計：用量是帳，不隨對話消失（UI 會說明）。

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scan::session_dirs;

/// 允許刪除的會話目錄（app, 目錄）。
#[derive(Debug, Clone)]
pub struct SessionEnv {
    pub roots: Vec<(String, PathBuf)>,
}

impl SessionEnv {
    /// 真實環境（命令層用）。測試建置直接拒絕。
    pub fn real() -> Result<Self, String> {
        if cfg!(test) {
            return Err(
                "測試中拒絕解析真實會話目錄 —— 請用 SessionEnv::for_test 傳暫存目錄".to_string(),
            );
        }
        let mut roots = vec![];
        for app in super::supported_apps() {
            for d in session_dirs(app)? {
                roots.push((app.to_string(), d));
            }
        }
        Ok(Self { roots })
    }

    /// 測試用：自己指定允許的目錄。
    #[cfg(test)]
    pub fn for_test(roots: Vec<(String, PathBuf)>) -> Self {
        Self { roots }
    }

    fn allows(&self, app: &str, real: &Path) -> bool {
        self.roots.iter().any(|(a, root)| {
            a == app
                && root
                    .canonicalize()
                    .map(|r| real.starts_with(&r))
                    .unwrap_or(false)
        })
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct DeleteOutcome {
    pub deleted: Vec<String>,
    /// (路徑, 原因)
    pub failed: Vec<(String, String)>,
    pub removed_bytes: u64,
    /// 給 UI 的提醒（例如「用量統計不會被撤銷」）
    pub note: String,
}

/// 刪除一批會話。`targets` 是 `(app, path)`。
pub fn delete_sessions(env: &SessionEnv, targets: &[(String, String)]) -> DeleteOutcome {
    let mut out = DeleteOutcome {
        note: "已刪除的對話不會從用量統計中扣除（用量是帳，不隨對話消失）".to_string(),
        ..Default::default()
    };
    for (app, path) in targets {
        match delete_one(env, app, path) {
            Ok(bytes) => {
                out.removed_bytes += bytes;
                out.deleted.push(path.clone());
            }
            Err(e) => out.failed.push((path.clone(), e)),
        }
    }
    out
}

fn delete_one(env: &SessionEnv, app: &str, path: &str) -> Result<u64, String> {
    if app == "opencode" {
        return Err("OpenCode 的會話存在它自己的資料庫裡，這裡只讀不刪".to_string());
    }
    let p = PathBuf::from(path);
    if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return Err("只接受 .jsonl 會話檔".to_string());
    }
    if !p.is_file() {
        return Err("檔案不存在（可能已經被刪掉了）".to_string());
    }
    let real = p.canonicalize().map_err(|e| format!("解析路徑失敗：{e}"))?;
    if !env.allows(app, &real) {
        return Err("路徑不在這個工具的會話目錄底下，拒絕刪除".to_string());
    }
    let size = std::fs::metadata(&real).map(|m| m.len()).unwrap_or(0);
    std::fs::remove_file(&real).map_err(|e| format!("刪除失敗：{e}"))?;
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn refuses_opencode_and_non_jsonl() {
        let env = SessionEnv::for_test(vec![]);
        let out = delete_sessions(
            &env,
            &[
                ("opencode".to_string(), "C:/x/opencode.db".to_string()),
                ("claude".to_string(), "C:/x/session.txt".to_string()),
                ("claude".to_string(), "C:/x/nope.jsonl".to_string()),
            ],
        );
        assert!(out.deleted.is_empty());
        assert_eq!(out.failed.len(), 3);
        assert!(out.failed[0].1.contains("只讀不刪"));
        assert!(out.failed[1].1.contains("只接受 .jsonl"));
        assert!(out.failed[2].1.contains("不存在"));
        assert!(out.note.contains("用量統計"), "要提醒用量不會被撤銷");
    }

    /// 會話目錄底下的檔案會真的被刪掉，並回報大小。
    #[test]
    fn inside_paths_are_deleted_with_size() {
        let root = tmp();
        let proj = root.path().join("projects").join("proj-a");
        std::fs::create_dir_all(&proj).unwrap();
        let f = proj.join("s1.jsonl");
        std::fs::write(&f, "{\"a\":1}\n").unwrap();
        let other = root.path().join("s2.jsonl");
        std::fs::write(&other, "x").unwrap();

        let env = SessionEnv::for_test(vec![(
            "claude".to_string(),
            root.path().join("projects"),
        )]);
        let out = delete_sessions(
            &env,
            &[
                ("claude".to_string(), f.to_string_lossy().to_string()),
                // 同一批裡有一個在允許目錄之外 → 它要被拒絕、且檔案還在
                ("claude".to_string(), other.to_string_lossy().to_string()),
            ],
        );
        assert_eq!(out.deleted.len(), 1, "{out:?}");
        assert_eq!(out.removed_bytes, 8);
        assert!(!f.exists(), "允許目錄底下的檔案要被刪掉");
        assert_eq!(out.failed.len(), 1);
        assert!(out.failed[0].1.contains("拒絕刪除"));
        assert!(other.exists(), "被拒絕的檔案必須還在");
    }

    /// 真實環境在測試建置裡要被拒絕。
    #[test]
    fn real_env_is_refused_under_test() {
        let e = SessionEnv::real();
        assert!(e.is_err());
        assert!(e.unwrap_err().contains("拒絕解析真實會話目錄"));
    }
}
