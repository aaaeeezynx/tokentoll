//! 檔案安全寫入工具（M1）。
//!
//! 策略與 cc-switch 一致：所有寫設定檔的操作走「臨時檔案 + rename」
//! 原子寫入，防止崩潰寫壞一半；備份目錄做輪換保留。

use std::fs;
use std::path::{Path, PathBuf};

/// 原子寫入檔案：先寫 `.tmp` 再 rename。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 當前毫秒時間戳（存 created_at / updated_at）。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 備份檔案名時間戳：YYYYMMDD-HHMMSS。
pub fn backup_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// 備份輪換：保留 `dir` 下屬於該 stem 的最近 `keep` 個檔案。
/// 檔名形如 `{stem}.bak-{stamp}`（同秒重複備份另有 `-{n}` 後綴，見 unique_backup_name）。
pub fn rotate_backups(dir: &Path, stem: &str, keep: usize) -> std::io::Result<()> {
    let prefix = format!("{stem}.bak");
    let mut olds: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with(&prefix) && (n.ends_with(".bak") || n.contains(".bak-"))
            })
        })
        .collect();
    olds.sort();
    while olds.len() > keep {
        let first = olds.remove(0);
        let _ = fs::remove_file(&first);
    }
    Ok(())
}

/// 同秒重複備份時保證檔名唯一（追加 `-2`、`-3`…），避免後寫覆蓋先寫、
/// 丟失真正的接管前原設定。後綴不影響還原時的"最新優先"排序。
pub fn unique_backup_name(dir: &Path, base: &str) -> PathBuf {
    let p = dir.join(base);
    if !p.exists() {
        return p;
    }
    let mut i = 2u32;
    loop {
        let q = dir.join(format!("{base}-{i}"));
        if !q.exists() {
            return q;
        }
        i += 1;
    }
}
