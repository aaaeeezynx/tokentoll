//! 版本探測：本機安裝偵測與帶超時的進程執行小工具。

use std::path::{Path, PathBuf};

use super::{npm_package, which_bin};

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


/// 把一條 pipe 交給執行緒讀到完（見 [`capture_with_timeout`] 的說明）。
fn spawn_reader<R: std::io::Read + Send + 'static>(
    r: Option<R>,
) -> Option<std::thread::JoinHandle<Vec<u8>>> {
    r.map(|mut s| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = std::io::Read::read_to_end(&mut s, &mut b);
            b
        })
    })
}

/// 一次捕獲的結果。
pub(crate) struct Captured {
    pub(crate) ok: bool,
    pub(crate) code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) timed_out: bool,
}

/// 跑一個外部命令並捕獲兩條輸出串流；超時就殺掉。
///
/// **為什麼要另開執行緒先讀 pipe**：OS 的 pipe 緩衝區只有 ~64 KB。舊寫法是等
/// 行程結束才 `wait_with_output()`，於是輸出大於緩衝區的指令會**寫滿 pipe 而
/// 阻塞**，永遠不結束 —— 每一輪都撞逾時。2026-10-08 由
/// `codex debug models --bundled`（658 KB）踩到：直接跑 2 秒，經過這個函式
/// 卻 30 秒逾時。
///
/// 逾時**不影響**已經讀到的內容：殺掉行程後仍會 join 兩條讀取執行緒。
pub(crate) fn capture_with_timeout(argv: &[String], timeout_ms: u64) -> Result<Captured, String> {
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
    let mut child = cmd.spawn().map_err(|e| format!("啟動失敗：{e}"))?;

    let h_out = spawn_reader(child.stdout.take());
    let h_err = spawn_reader(child.stderr.take());

    let step = 200u64;
    let mut waited = 0u64;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait().map_err(|e| format!("等待失敗：{e}"))? {
            Some(s) => break s,
            None => {
                if waited >= timeout_ms {
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().map_err(|e| format!("等待失敗：{e}"))?;
                }
                std::thread::sleep(std::time::Duration::from_millis(step));
                waited += step;
            }
        }
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.map(|h| h.join().unwrap_or_default()).unwrap_or_default()
    };
    Ok(Captured {
        ok: status.success(),
        code: status.code(),
        stdout: join(h_out),
        stderr: join(h_err),
        timed_out,
    })
}

/// 帶超時執行（輪詢等待，超時殺進程；不經 shell，argv[0] 必須是真 exe 或 cmd）。
pub(crate) fn run_with_timeout(argv: &[String], timeout_ms: u64) -> Result<String, String> {
    let c = capture_with_timeout(argv, timeout_ms)?;
    if c.timed_out {
        return Err(format!("超時（>{timeout_ms}ms），已終止"));
    }
    if !c.ok {
        return Err(format!("退出碼非零：{}", c.code.unwrap_or(-1)));
    }
    let mut text = String::from_utf8_lossy(&c.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&c.stderr).to_string();
    }
    Ok(text)
}


/// 寬鬆捕獲（版本探測用）：不要求退出碼為零，只要有輸出就算數
///（部分 CLI 在有更新可用時 `--version` 非零退出）。
pub(crate) fn run_capture_lenient(argv: &[String], timeout_ms: u64) -> Option<String> {
    let c = capture_with_timeout(argv, timeout_ms).ok()?;
    if c.timed_out {
        return None;
    }
    let mut text = String::from_utf8_lossy(&c.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&c.stderr).to_string();
    }
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
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
