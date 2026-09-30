//! 主題（P4.3）：深色／淺色／跟隨系統。
//!
//! ## 我們的作法
//!
//! 前端用 **CSS 變數**當唯一來源：`<html data-theme="dark|light">` 決定一整組
//! token（`--mac-window`／`--mac-label`／`--fg`…），Tailwind 透過 `@theme`
//! 把 `--color-fg` 對應成 `text-fg` / `bg-fg` / `border-fg`，
//! 所以「白色」在淺色主題會自動變成深色文字（`text-fg/60` 這種透明度寫法也照用）。
//!
//! 「跟隨系統」不是把 `data-theme` 設成 system，而是**由前端監聽
//! `prefers-color-scheme` 後寫入實際的 dark/light** —— 這樣 CSS 只需要兩種狀態，
//! 而系統切換時也會即時跟著變。
//!
//! 偏好存在 `settings` 表（`theme`），因此三個選項都會被保留（包含 system）。
//! 後端只負責存取，真正的套用在前端。

/// 合法的主題選項（cc-switch：System／Light／Dark）。
pub const THEMES: &[&str] = &["system", "light", "dark"];

/// 驗證主題值。
pub fn validate(theme: &str) -> Result<String, String> {
    let t = theme.trim().to_ascii_lowercase();
    if THEMES.contains(&t.as_str()) {
        Ok(t)
    } else {
        Err(format!("不支援的主題「{theme}」（可用：{}）", THEMES.join("／")))
    }
}

/// 依偏好與系統狀態決定實際要套用的主題（`system` → 看系統）。
///
/// 實際的解析在前端（它有 `matchMedia` 可以監聽系統變化）；這裡保留同一個
/// 語意並以測試釘住，避免「跟隨系統」的定義前後端各說一套。
#[cfg(test)]
pub fn resolve(pref: &str, system_dark: bool) -> &'static str {
    match pref.trim().to_ascii_lowercase().as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => {
            if system_dark {
                "dark"
            } else {
                "light"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_the_three_options() {
        for ok in ["system", "light", "dark", " DARK "] {
            assert!(validate(ok).is_ok(), "{ok} 應該可以");
        }
        assert_eq!(validate(" DARK ").unwrap(), "dark");
        assert!(validate("solarized").unwrap_err().contains("不支援的主題"));
    }

    #[test]
    fn resolve_follows_system_only_for_system() {
        assert_eq!(resolve("light", true), "light", "明講淺色就不管系統");
        assert_eq!(resolve("dark", false), "dark", "明講深色就不管系統");
        assert_eq!(resolve("system", true), "dark");
        assert_eq!(resolve("system", false), "light");
        assert_eq!(resolve("", false), "light", "空值視為跟隨系統");
    }
}
