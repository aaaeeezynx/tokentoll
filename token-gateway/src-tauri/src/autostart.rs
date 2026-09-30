//! 開機自啟（P4.4，對標 cc-switch 的 "Launch on Startup"）。
//!
//! Windows 上用**登錄檔的 Run 機碼**（cc-switch 手冊也是這樣寫的）：
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 底下的字串值
//! `TokenGateway` = `"C:\…\token-gateway.exe"`。
//!
//! 選 HKCU 而不是 HKLM：**不需要系統管理員**，而且與「這個 App 是每個使用者
//! 自己的工具」的定位一致（與 Deep Link 的協定註冊同一條原則）。
//!
//! 狀態**以登錄檔為準**（不另外存一份在資料庫）：使用者也許用工作管理員或
//! 其他工具關掉它，我們不該顯示一個自己想像的狀態。

use std::path::Path;

/// Run 機碼底下的值名稱。
const VALUE: &str = "TokenGateway";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| format!("執行 reg 失敗：{e}"))
}

/// 目前註冊的啟動命令（沒註冊就 None）。
pub fn registered_command() -> Option<String> {
    let out = reg(&["query", RUN_KEY, "/v", VALUE]).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|l| l.contains("REG_SZ"))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 打開／關閉開機自啟。回傳設定後是否為啟用。
pub fn set_enabled(enabled: bool, exe: &Path) -> Result<bool, String> {
    if enabled {
        let cmd = format!("\"{}\"", exe.to_string_lossy());
        let out = reg(&["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])?;
        if !out.status.success() {
            return Err(format!(
                "寫入啟動機碼失敗：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(true)
    } else {
        // 已經沒有這個值時 reg delete 會回非零：那不是錯誤（目標已達成）
        let out = reg(&["delete", RUN_KEY, "/v", VALUE, "/f"])?;
        if out.status.success() {
            return Ok(false);
        }
        if registered_command().is_none() {
            return Ok(false);
        }
        Err(format!(
            "移除啟動機碼失敗：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只讀目前狀態；測試不會去改使用者的啟動設定。
    #[test]
    fn query_is_side_effect_free() {
        let a = registered_command();
        let b = registered_command();
        assert_eq!(a, b);
        // 有註冊的話，命令一定要指向一個 exe（引號包起來）
        if let Some(cmd) = a {
            assert!(cmd.to_lowercase().contains(".exe"), "{cmd}");
        }
    }

    /// 值名稱與機碼路徑是照 cc-switch 手冊的作法（Windows 用登錄檔）。
    #[test]
    fn constants_are_the_expected_registry_location() {
        assert!(RUN_KEY.contains("CurrentVersion\\Run"));
        assert_eq!(VALUE, "TokenGateway");
    }
}
