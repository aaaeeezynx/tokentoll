//! 版本比較與行為樁
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;


#[test]
fn version_cmp_and_clean() {
    use std::cmp::Ordering;
    assert_eq!(clean_version("codex-cli 0.153.4"), Some("0.153.4".to_string()));
    assert_eq!(clean_version("2.1.263 (Claude Code)"), Some("2.1.263".to_string()));
    assert_eq!(
        clean_version("Hermes Agent v0.21.1 (2026.9.7) · upstream abc"),
        Some("0.21.1".to_string())
    );
    assert_eq!(clean_version("0.1.2-rc.1"), Some("0.1.2-rc.1".to_string()));
    assert_eq!(clean_version("no version here"), None);
    assert_eq!(cmp_version("1.2.10", "1.2.9"), Ordering::Greater);
    assert_eq!(cmp_version("2.1.263", "2.1.263"), Ordering::Equal);
    assert_eq!(cmp_version("0.1.2", "0.1.2-rc.1"), Ordering::Greater);
    assert_eq!(cmp_version("0.1.2-rc.1", "0.1.2"), Ordering::Less);
    assert_eq!(cmp_version("0.153.4", "0.153.10"), Ordering::Less);
}


#[test]
fn npm_pkg_version_reads_package_json() {
    // scoped 包路徑 @a\b + version 字段解析。
    let dir = tempfile::tempdir().unwrap();
    let pkgdir = dir.path().join("@anthropic-ai").join("claude-code");
    std::fs::create_dir_all(&pkgdir).unwrap();
    std::fs::write(pkgdir.join("package.json"), r#"{"name":"x","version":"2.1.263"}"#).unwrap();
    assert_eq!(
        npm_pkg_version_at(dir.path(), "@anthropic-ai/claude-code"),
        Some("2.1.263".to_string())
    );
    assert_eq!(npm_pkg_version_at(dir.path(), "nope"), None);
}


#[test]
#[cfg(windows)]
fn run_timeout_kills_hang() {
    // 回顯成功。
    let ok = run_with_timeout(
        &["cmd".to_string(), "/C".to_string(), "echo".to_string(), "hi".to_string()],
        10_000,
    )
    .unwrap();
    assert!(ok.contains("hi"), "{ok}");
    // ping 5 秒，800ms 限：必須超時殺掉。
    let err = run_with_timeout(
        &["cmd".to_string(), "/C".to_string(), "ping".to_string(), "-n".to_string(), "6".to_string(), "127.0.0.1".to_string()],
        800,
    )
    .unwrap_err();
    assert!(err.contains("超時"), "{err}");
}
