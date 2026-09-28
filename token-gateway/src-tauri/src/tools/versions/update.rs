//! 一鍵更新與 registry 最新版查詢。

use super::probe::{npm_global_root, npm_pkg_version_at, probe_tool_version};
use super::version_cmp::{clean_version, cmp_version};
use super::{
    first_line, npm_package, run_capture_lenient, run_with_timeout, which_bin, ToolLatest,
    ToolVersion,
};

/// 單工具更新命令（argv 直接可執行；npm 走 `cmd /C npm …`）。
pub(crate) fn tool_update_argv(app: &str) -> (Vec<String>, Option<String>) {
    if let Some(pkg) = npm_package(app) {
        if which_bin(&[app]).is_none() {
            return (vec![], None);
        }
        return (
            vec![
                "cmd".to_string(),
                "/C".to_string(),
                "npm".to_string(),
                "i".to_string(),
                "-g".to_string(),
                format!("{pkg}@latest"),
            ],
            Some(format!("npm i -g {pkg}@latest")),
        );
    }
    (vec![], None)
}


/// registry 最新版（`npm view <pkg> version`，30 秒超時；斷網/超時回 None）。
pub(crate) fn npm_latest(pkg: &str) -> Option<String> {
    let argv = vec![
        "cmd".to_string(),
        "/C".to_string(),
        "npm".to_string(),
        "view".to_string(),
        pkg.to_string(),
        "version".to_string(),
    ];
    let out = run_capture_lenient(&argv, 30_000)?;
    first_line(&out)
}


pub fn tool_versions() -> Vec<ToolVersion> {
    // 第一階段 A／F：只列「有接管/檢測意義」的工具（hermes 於 F 移除）。
    const APPS: [(&str, &str); 4] = [
        ("claude", "Claude Code"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("dsh", "DeepSeek Harness"),
    ];
    APPS.iter()
        .map(|(app, display)| {
            let (installed, version) = probe_tool_version(app);
            let (update_argv, update_label) = if installed {
                tool_update_argv(app)
            } else {
                (vec![], None)
            };
            ToolVersion {
                app: app.to_string(),
                display: display.to_string(),
                installed,
                version,
                update_argv,
                update_label,
                up_to_date: None,
            }
        })
        .collect()
}


/// 各 npm 包最新版（並行查 registry）。
/// 前端獨立調用，不擋版本顯示。
pub fn tool_latest() -> Vec<ToolLatest> {
    const NPM_APPS: [&str; 4] = ["claude", "codex", "opencode", "dsh"];
    // 本機已裝版本（讀 package.json，不起進程；缺失時回退跑 --version）。
    let mut cur: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    #[cfg(windows)]
    if let Some(root) = npm_global_root() {
        for app in NPM_APPS {
            if let Some(pkg) = npm_package(app) {
                if let Some(v) = npm_pkg_version_at(&root, pkg) {
                    cur.insert(app, v);
                }
            }
        }
    }
    for app in NPM_APPS {
        if !cur.contains_key(app) {
            if let (_, Some(v)) = probe_tool_version(app) {
                cur.insert(app, clean_version(&v).unwrap_or(v));
            }
        }
    }
    // 並行問 registry。
    let latest: std::collections::HashMap<&str, Option<String>> = std::thread::scope(|s| {
        let handles: Vec<(&str, std::thread::ScopedJoinHandle<Option<String>>)> = NPM_APPS
            .iter()
            .map(|app| {
                let pkg = npm_package(app).expect("npm app");
                (*app, s.spawn(move || npm_latest(pkg)))
            })
            .collect();
        handles
            .into_iter()
            .map(|(app, h)| (app, h.join().unwrap_or(None)))
            .collect()
    });
    let out: Vec<ToolLatest> = NPM_APPS
        .iter()
        .map(|app| {
            let l = latest.get(app).cloned().flatten();
            let up = match (cur.get(app), &l) {
                (Some(c), Some(n)) => {
                    Some(cmp_version(c, n) != std::cmp::Ordering::Less)
                }
                _ => None,
            };
            ToolLatest {
                app: app.to_string(),
                latest: l,
                up_to_date: up,
            }
        })
        .collect();
    out
}


/// 執行一鍵更新（600 秒超時；返回輸出尾部；npm 走自家官方通道）。
pub fn tool_update_run(app: &str) -> Result<String, String> {
    let known = ["claude", "codex", "opencode", "dsh"];
    if !known.contains(&app) {
        return Err(format!("未知工具：{app}"));
    }
    let (argv, label) = tool_update_argv(app);
    if argv.is_empty() {
        return Err("該工具未安裝或不支援一鍵更新".to_string());
    }
    let out = run_with_timeout(&argv, 600_000)?;
    // 只回輸出尾部（npm 日誌可達數百行）。
    let tail: String = out.lines().rev().take(25).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
    Ok(format!("{} 執行完畢：\n{tail}", label.unwrap_or_default()))
}
