//! `updater` 的測試（版本比較、manifest 解析、結果組成）。

use super::check::{
    build_result, compare_version, current_version, last_result, parse_manifest, platform_key,
    record, url_setting, Manifest, PlatformEntry,
};
use super::URL_KEY;

fn manifest_json(version: &str, platforms: &str, notes: &str, date: &str) -> String {
    format!(
        r#"{{"version":"{version}","notes":"{notes}","pub_date":"{date}","platforms":{platforms}}}"#
    )
}

#[test]
fn version_compare_handles_prefixes_and_lengths() {
    assert_eq!(compare_version("0.2.0", "0.1.0"), 1);
    assert_eq!(compare_version("0.1.0", "0.2.0"), -1);
    assert_eq!(compare_version("0.1.0", "0.1.0"), 0);
    assert_eq!(compare_version("v1.0.0", "1.0.0"), 0, "v 前綴不影響");
    assert_eq!(compare_version("0.2", "0.2.0"), 0, "段數不同但數字一樣");
    assert_eq!(compare_version("1.0.0", "0.9.9"), 1);
    assert_eq!(compare_version("1.0.0-beta", "1.0.0"), 0, "不比 pre-release（已寫在文件）");
    assert_eq!(compare_version("2", "10"), -1, "不是字串比較");
}

#[test]
fn platform_key_matches_tauri_naming() {
    let k = platform_key();
    assert!(k.contains('-'), "{k}");
    if cfg!(target_os = "windows") {
        assert!(k.starts_with("windows-"), "{k}");
    }
}

#[test]
fn parses_a_real_manifest() {
    let text = manifest_json(
        "0.2.0",
        r#"{"windows-x86_64":{"signature":"abc","url":"https://x/setup.exe"}}"#,
        "修正了幾個問題",
        "2026-01-01T00:00:00Z",
    );
    let m = parse_manifest(&text).unwrap();
    assert_eq!(m.version, "0.2.0");
    assert_eq!(m.notes, "修正了幾個問題");
    assert_eq!(m.pub_date, "2026-01-01T00:00:00Z");
    assert_eq!(m.platforms.len(), 1);
    assert_eq!(m.platforms["windows-x86_64"].url, "https://x/setup.exe");
    assert_eq!(m.platforms["windows-x86_64"].signature, "abc");
}

#[test]
fn rejects_broken_manifests_with_readable_errors() {
    assert!(parse_manifest("not json").unwrap_err().contains("格式不對"));
    assert!(parse_manifest("{}").unwrap_err().contains("沒有 version"));
    assert!(parse_manifest(r#"{"version":"  "}"#).unwrap_err().contains("沒有 version"));
}

#[test]
fn result_reports_newer_version_with_link() {
    // 假裝現在是 0.1.0：直接測 build_result 的判斷分支
    let m = Manifest {
        version: "9.9.9".into(),
        notes: "n".into(),
        pub_date: "d".into(),
        platforms: [(
            platform_key(),
            PlatformEntry {
                signature: "sig".into(),
                url: "https://x/setup.exe".into(),
            },
        )]
        .into_iter()
        .collect(),
    };
    let r = build_result(&m);
    assert!(r.has_update);
    assert_eq!(r.url, "https://x/setup.exe");
    assert!(r.signed);
    assert!(r.message.contains("有新版 9.9.9"), "{}", r.message);
    assert!(r.message.contains("已附簽章"), "{}", r.message);
}

#[test]
fn result_flags_same_version_and_missing_platform() {
    // 與目前版本相同 → 已是最新
    let same = Manifest {
        version: current_version(),
        ..Default::default()
    };
    let r = build_result(&same);
    assert!(!r.has_update);
    assert!(r.message.contains("已是最新"), "{}", r.message);
    assert!(r.url.is_empty());

    // 有新版但沒有這個平台的檔案 → 要講清楚
    let no_platform = Manifest {
        version: "99.0.0".into(),
        platforms: [(
            "linux-aarch64".to_string(),
            PlatformEntry {
                signature: String::new(),
                url: "https://x/appimage".into(),
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let r2 = build_result(&no_platform);
    assert!(r2.has_update);
    assert!(r2.url.is_empty());
    assert!(r2.message.contains("沒有"), "{}", r2.message);
    assert!(r2.message.contains(platform_key().split('-').next().unwrap()), "{}", r2.message);
}

#[test]
fn unsigned_manifest_is_called_out() {
    let m = Manifest {
        version: "99.1.0".into(),
        platforms: [(
            platform_key(),
            PlatformEntry {
                signature: String::new(), // 沒簽章
                url: "https://x/setup.exe".into(),
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let r = build_result(&m);
    assert!(!r.signed);
    assert!(r.message.contains("沒有附簽章"), "{}", r.message);
}

#[test]
fn settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
    assert!(url_setting(&conn).is_empty());
    crate::db::set_setting(&conn, URL_KEY, "https://example.com/latest.json").unwrap();
    assert_eq!(url_setting(&conn), "https://example.com/latest.json");
    assert_eq!(last_result(&conn).0, 0);
    record(&conn, "已是最新").unwrap();
    let (ms, msg) = last_result(&conn);
    assert!(ms > 0);
    assert_eq!(msg, "已是最新");
}
