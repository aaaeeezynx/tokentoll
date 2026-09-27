//! 版本探測與更新
//!
//! 由 `tools.rs` 拆分而來（原檔 4,266 行）。**程式碼一字未改**，只搬位置。
//! 共用前置集中在父模組 `tools`，子模組以 `use super::*;` 取用。

use super::*;

/// 本機工具版本 + 一鍵更新資訊（版本頁用；與 10 秒輪詢的 detect 分開，避免頻繁起進程）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolVersion {
    pub app: String,
    pub display: String,
    pub installed: bool,
    /// 版本字串（取首行），未安裝或取不到時為 None。
    pub version: Option<String>,
    /// 更新命令 argv（不經 shell，直接執行；為空 = 不支援一鍵更新）。
    pub update_argv: Vec<String>,
    /// 更新命令的人類可讀展示。
    pub update_label: Option<String>,
    /// 是否已是最新（由 `tool_latest` 填；`tool_versions` 一律留 None）。
    pub up_to_date: Option<bool>,
}


/// 最新版查詢結果（走 registry，有網絡延遲；前端獨立查詢，不擋版本顯示）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolLatest {
    pub app: String,
    pub latest: Option<String>,
    pub up_to_date: Option<bool>,
}


/// npm 全局包名（claude/codex/opencode/dsh 走 npm 更新）。
pub(crate) fn npm_package(app: &str) -> Option<&'static str> {
    match app {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "opencode" => Some("opencode-ai"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}


/// 在 PATH 中解析可執行檔完整路徑（Windows 含常見副檔名）。
pub(crate) fn which_bin(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for d in std::env::split_paths(&path) {
        for n in names {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
            #[cfg(windows)]
            for ext in ["exe", "cmd", "ps1"] {
                let q = d.join(format!("{n}.{ext}"));
                if q.is_file() {
                    return Some(q);
                }
            }
        }
    }
    None
}


/// 讀 npm 全局包 package.json 的 version（純讀檔，不起進程，最快最穩）。
pub(crate) fn npm_pkg_version_at(root: &Path, pkg: &str) -> Option<String> {
    // scoped 包 `@a/b` → node_modules\@a\b
    let rel: PathBuf = pkg.split('/').collect();
    let text = std::fs::read_to_string(root.join(rel).join("package.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("version")?
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}


#[cfg(windows)]
pub(crate) fn npm_global_root() -> Option<PathBuf> {
    // npm 默認全局根 %APPDATA%\npm\node_modules；解析 npm.cmd 位置做二次確認。
    let appdata = dirs::data_dir()?;
    let root = appdata.join("npm").join("node_modules");
    if root.is_dir() {
        return Some(root);
    }
    which_bin(&["npm"]).and_then(|p| {
        p.parent()?
            .join("node_modules")
            .is_dir()
            .then(|| p.parent().expect("parent").join("node_modules"))
    })
}


/// 帶超時執行（輪詢等待，超時殺進程；不經 shell，argv[0] 必須是真 exe 或 cmd）。
pub(crate) fn run_with_timeout(argv: &[String], timeout_ms: u64) -> Result<String, String> {
    if argv.is_empty() {
        return Err("空命令".to_string());
    }
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW：避免彈黑框
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("啟動失敗：{e}"))?;
    let step = 200u64;
    let mut waited = 0u64;
    loop {
        match child.try_wait().map_err(|e| format!("等待失敗：{e}"))? {
            Some(status) => {
                if !status.success() {
                    return Err(format!("退出碼非零：{}", status.code().unwrap_or(-1)));
                }
                let out = child
                    .wait_with_output()
                    .map_err(|e| format!("讀輸出失敗：{e}"))?;
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stderr).to_string();
                }
                return Ok(text);
            }
            None => {
                if waited >= timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("超時（>{timeout_ms}ms），已終止"));
                }
                std::thread::sleep(std::time::Duration::from_millis(step));
                waited += step;
            }
        }
    }
}


/// 寬鬆捕獲（版本探測用）：不要求退出碼為零，只要有輸出就算數
///（部分 CLI 在有更新可用時 `--version` 非零退出）。
pub(crate) fn run_capture_lenient(argv: &[String], timeout_ms: u64) -> Option<String> {
    if argv.is_empty() {
        return None;
    }
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW：避免彈黑框
    }
    let mut child = cmd.spawn().ok()?;
    let step = 200u64;
    let mut waited = 0u64;
    loop {
        match child.try_wait().ok()? {
            Some(_) => {
                let out = child.wait_with_output().ok()?;
                let mut text = String::from_utf8_lossy(&out.stdout).to_string();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stderr).to_string();
                }
                return if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                };
            }
            None => {
                if waited >= timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(step));
                waited += step;
            }
        }
    }
}


pub(crate) fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| {
            let mut t = l.to_string();
            if t.len() > 120 {
                t.truncate(120);
            }
            t
        })
}


/// 單工具版本探測（20 秒超時；npm 包優先讀 package.json）。
pub(crate) fn probe_tool_version(app: &str) -> (bool, Option<String>) {
    if let Some(pkg) = npm_package(app) {
        #[cfg(windows)]
        if let Some(root) = npm_global_root() {
            if let Some(v) = npm_pkg_version_at(&root, pkg) {
                return (true, Some(v));
            }
        }
        // 回退：跑二進制 --version（.ps1/.cmd 走 powershell/cmd）。
        if let Some(bin) = which_bin(&[app]) {
            let argv: Vec<String> = match bin.extension().and_then(|e| e.to_str()) {
                Some("ps1") => vec![
                    "powershell".to_string(),
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                    format!("& '{}' --version", bin.to_string_lossy()),
                ],
                Some("cmd") | Some("bat") => vec![
                    "cmd".to_string(),
                    "/C".to_string(),
                    bin.to_string_lossy().to_string(),
                    "--version".to_string(),
                ],
                _ => vec![bin.to_string_lossy().to_string(), "--version".to_string()],
            };
            if let Some(out) = run_capture_lenient(&argv, 20_000) {
                if let Some(v) = first_line(&out) {
                    return (true, Some(v));
                }
            }
            return (true, None);
        }
        return (false, None);
    }
    // hermes（第一階段 F 移除）與未知工具都走這裡：沒有可問版本的二進制。
    (false, None)
}


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


/// 版本段式比較（`1.2.10 > 1.2.9`；無後綴 > 有後綴，如 `0.1.2 > 0.1.2-rc.1`）。
pub(crate) fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |s: &str| {
        let (nums, pre) = match s.split_once('-') {
            Some((n, p)) => (n, Some(p.to_string())),
            None => (s, None),
        };
        let ns: Vec<u64> = nums.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        (ns, pre)
    };
    let (mut an, ap) = split(a.trim());
    let (mut bn, bp) = split(b.trim());
    let n = an.len().max(bn.len());
    an.resize(n, 0);
    bn.resize(n, 0);
    match an.cmp(&bn) {
        Ordering::Equal => match (ap, bp) {
            (None, None) => Ordering::Equal,
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(x), Some(y)) => x.cmp(&y),
        },
        ord => ord,
    }
}


/// 從人類可讀版本行提純番號（`codex-cli 0.153.4`→`0.153.4`，
/// `0.1.2-rc.1` 保留後綴）。
pub(crate) fn clean_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            if j < b.len() && b[j] == b'-' {
                let mut k = j + 1;
                while k < b.len()
                    && (b[k].is_ascii_alphanumeric() || b[k] == b'.' || b[k] == b'-')
                {
                    k += 1;
                }
                if k > j + 1 {
                    j = k;
                }
            }
            let cand = &s[i..j];
            if cand.contains('.') {
                return Some(cand.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
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
