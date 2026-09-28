//! 備份、baseline 與還原
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

/// 備份是否為本工具的網關接管產物（還原時應跳過）。
pub(crate) fn backup_is_tainted(app: &str, text: &str, port: u16) -> bool {
    match app {
        "codex" => codex_text_base_url(text)
            .or_else(|| {
                text.parse::<toml_edit::DocumentMut>().ok().and_then(|doc| {
                    ["custom", GATEWAY_PROVIDER_ID]
                        .into_iter()
                        .find_map(|id| {
                            doc.get("model_providers")
                                .and_then(|p| p.get(id))
                                .and_then(|p| p.get("base_url"))
                                .and_then(|u| u.as_str())
                                .map(|u| u.to_string())
                        })
                })
            })
            .is_some_and(|u| is_gateway(&u, port)),
        "claude" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("env")
                    .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                    .and_then(|u| u.as_str())
                    .map(|u| is_gateway(u, port))
            })
            .unwrap_or(false),
        "opencode" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("provider")
                    .and_then(|p| p.get(GATEWAY_PROVIDER_ID))
                    .and_then(|p| p.get("options"))
                    .and_then(|o| o.get("baseURL"))
                    .and_then(|u| u.as_str())
                    .map(|u| is_gateway(u, port))
            })
            .unwrap_or(false),
        _ => false,
    }
}


/// 備份輪換（保乾淨版）：總數超限時優先淘汰最舊的接管態備份，
/// 乾淨（接管前）備份留到最後才動——它是關閉接管的退路之一。
/// 全是乾淨備份卻超限時才退化為淘汰最舊者（與舊邏輯一致）。
///
/// `keep = 1`（第一階段 E）時的行為：先刪污染備份、再刪最舊的，最後留下的一定
/// 是最有價值的那一份。**baseline 檔（`{stem}.baseline-*`）不在本函式的掃描
/// 範圍內，永遠不會被這裡刪掉**（那是另一條、更重要的退路）。
pub(crate) fn prune_backups_keep_clean(
    dir: &Path,
    app: &str,
    stem: &str,
    keep: usize,
    port: u16,
) -> std::io::Result<()> {
    let prefix = format!("{stem}.bak-");
    let mut olds: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    olds.sort();
    while olds.len() > keep {
        let victim = olds
            .iter()
            .position(|p| {
                std::fs::read_to_string(p)
                    .map(|t| backup_is_tainted(app, &t, port))
                    .unwrap_or(false)
            })
            .unwrap_or(0);
        let v = olds.remove(victim);
        let _ = std::fs::remove_file(&v);
    }
    Ok(())
}


/// 第一階段 E 的安全網：在「只留 1 份」的輪換**之前**，先確保接管前的乾淨
/// 基準備份存在。
///
/// 為什麼需要它：`prune_backups_keep_clean(keep = 1)` 會把舊的 `bak-` 砍掉。
/// 對「沒有 baseline、只有一串歷史備份」的工具目錄（實際上 claude／opencode
/// 就是這樣）而言，**最初的原始設定可能只存在於最舊的那份 bak 裡**，
/// 一旦被砍就再也還原不回去。所以輪換前先做一次基準備份：
///
/// - 當前內容本來就乾淨 → 直接用它寫 baseline（原本 `apply_switch` 的行為）
/// - 當前內容是接管態 → 從歷史備份裡找**最新的乾淨備份**遷移成 baseline
///   （重用 `migrate_gateway_baseline` 的掃描邏輯）
///
/// 失敗時刻意**不報錯**（例如整串備份都含網關痕跡，那本來就還原不了）：
/// 這是盡力而為的保險，不該讓接管本身失敗。
pub(crate) fn ensure_baseline_before_prune(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    old: &str,
    port: u16,
) {
    let stem = cfg.file_name().and_then(|n| n.to_str()).unwrap_or("config");
    let dir = app_data.join("backups").join(app);
    if has_baseline(&dir, stem) {
        return;
    }
    if !backup_is_tainted(app, old, port) {
        let _ = write_baseline(&dir, stem, old);
        return;
    }
    let _ = migrate_gateway_baseline(app_data, app, cfg, port);
}


pub(crate) fn backup_has_known_route(app: &str, text: &str) -> bool {
    match app {
        "codex" => codex_text_base_url(text).is_some(),
        "claude" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("env")
                    .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                    .and_then(|u| u.as_str())
                    .map(|_| true)
            })
            .unwrap_or(false),
        "opencode" => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|v| {
                v.get("provider")
                    .and_then(|p| p.get(GATEWAY_PROVIDER_ID))
                    .and_then(|p| p.get("options"))
                    .and_then(|o| o.get("baseURL"))
                    .and_then(|u| u.as_str())
                    .map(|_| true)
            })
            .unwrap_or(false),
        _ => false,
    }
}


pub(crate) fn backup_files(dir: &Path, stem: &str) -> std::io::Result<Vec<PathBuf>> {
    let prefix = format!("{stem}.bak-");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    files.sort();
    Ok(files)
}


pub(crate) fn has_baseline(dir: &Path, stem: &str) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.baseline-"))
            })
        })
        .unwrap_or(false)
}


pub(crate) fn remove_stale_baselines(dir: &Path, stem: &str) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for p in rd.filter_map(|e| e.ok()).map(|e| e.path()) {
            if p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{stem}.baseline-")))
            {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}


pub(crate) fn write_baseline(dir: &Path, stem: &str, text: &str) -> std::io::Result<PathBuf> {
    remove_stale_baselines(dir, stem);
    let path = dir.join(format!(
        "{stem}.baseline-{}",
        crate::fsutil::backup_stamp()
    ));
    std::fs::write(&path, text)?;
    Ok(path)
}


pub(crate) fn migrate_gateway_baseline(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
) -> Result<(), String> {
    let stem = cfg
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let dir = app_data.join("backups").join(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if has_baseline(&dir, stem) {
        let mut existing = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{stem}.baseline-"))
            })
            .collect::<Vec<_>>();
            existing.sort_by_key(|e| e.file_name());
            if let Some(path) = existing.last() {
                let text = std::fs::read_to_string(path.path()).map_err(|e| e.to_string())?;
                if backup_has_known_route(app, &text) && !backup_is_tainted(app, &text, port) {
                    return Ok(());
                }
            }
    }
    remove_stale_baselines(&dir, stem);
    let current = std::fs::read_to_string(cfg).unwrap_or_default();
    if backup_has_known_route(app, &current) && !backup_is_tainted(app, &current, port) {
        write_baseline(&dir, stem, &current).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mut candidates = backup_files(&dir, stem).map_err(|e| e.to_string())?;
    candidates.reverse();
    for path in candidates {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if backup_has_known_route(app, &text) && !backup_is_tainted(app, &text, port) {
            write_baseline(&dir, stem, &text).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("所有備份都還含網關配置，無法還原到接管前".into())
}


/// 一鍵還原：把接管前的備份寫回配置文件。
/// 從新到舊掃描 `{stem}.bak-*`，跳過仍含網關痕跡的版本（多次切換會產生多層備份）。
///
/// 註：原本另有 `restore_backup_to(...)`（無 port 參數）只被測試呼叫，且它傳
/// `migrate: false`，是「不做 baseline 遷移」的變體（B7）。為免生產 API 出現
/// 兩個語義不同的同名還原入口，已把它降為測試模組內的 helper；生產呼叫端一律
/// 走本函式（migrate=true）。
pub fn restore_backup_to_port(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
) -> Result<String, String> {
    restore_backup_to_inner(app_data, app, cfg, port, true)
}


pub(crate) fn restore_backup_to_inner(
    app_data: &Path,
    app: &str,
    cfg: &Path,
    port: u16,
    migrate: bool,
) -> Result<String, String> {
    if !["claude", "codex", "opencode"].contains(&app) {
        return Err("該工具不支援一鍵還原".into());
    }
    let stem = cfg
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    if migrate {
        if let Err(e) = migrate_gateway_baseline(app_data, app, cfg, port) {
            if has_baseline(
                &app_data.join("backups").join(app),
                cfg.file_name().and_then(|n| n.to_str()).unwrap_or("config"),
            ) {
                let _ = e;
            } else {
                return Err(e);
            }
        }
    }
    let dir = app_data.join("backups").join(app);
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|_| "找不到該工具的備份目錄，無法還原".to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| {
                    n.starts_with(&format!("{stem}.bak-"))
                        || n.starts_with(&format!("{stem}.baseline-"))
                })
        })
        .collect();
    if cands.is_empty() {
        return Err("找不到接管前的備份，無法還原".into());
    }
    cands.sort();
    cands.reverse();
    let mut skipped = 0usize;
    for c in &cands {
        let text = std::fs::read_to_string(c).map_err(|e| format!("備份讀取失敗：{e}"))?;
        // 跳過接管產物（provider 段名 / 網關本地端口）；用戶指向其他本機服務的原設定不在此列。
        if backup_is_tainted(app, &text, port) {
            skipped += 1;
            continue;
        }
        crate::fsutil::atomic_write(cfg, text.as_bytes())
            .map_err(|e| format!("還原寫入失敗：{e}"))?;
        // 透明還原：告知還原自哪個備份、跳過幾個、內容摘要；死鏈（直連 Chat 上游卻走 responses）明確警告。
        let name = c
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let summary = restore_summary(&text);
        let skip_note = if skipped > 0 {
            format!("（跳過 {skipped} 個含網關痕跡的備份）")
        } else {
            String::new()
        };
        // 還原到 cc-switch 託管態：能用才算數（需啟動 cc-switch），明確告知。
        let switch_note = if text.contains("127.0.0.1:15721") || text.contains("localhost:15721") {
            "｜⚠️ 還原後走 cc-switch（15721），請先啟動 cc-switch，否則工具連不上"
        } else {
            ""
        };
        if migrate {
            remove_stale_baselines(&dir, stem);
        }
        return Ok(format!(
            "{}｜還原自 {name}{skip_note}{switch_note}｜{summary}",
            cfg.to_string_lossy()
        ));
    }
    Err("所有備份都還含網關配置，無法還原到接管前".into())
}


/// 還原指定工具到接管前狀態（埠由呼叫端提供）。
/// 註：原本另有 `restore_backup(app_data, app)` 預設埠包裝，只被測試呼叫而成為
/// dead_code（B7），已降為測試模組內的 helper。
pub fn restore_backup_port(app_data: &Path, app: &str, port: u16) -> Result<String, String> {
    let cfg = config_path_for(app)?;
    restore_backup_to_port(app_data, app, &cfg, port)
}

// ---------------------------------------------------------------- 單測 ---
