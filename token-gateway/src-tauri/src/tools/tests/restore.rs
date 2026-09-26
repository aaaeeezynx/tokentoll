//! 備份、還原與 prune
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;


#[test]
fn restore_uses_effective_codex_provider_url() {
    let direct = "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
    let managed = "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n\n[model_providers.tokengateway]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
    assert!(!backup_is_tainted("codex", direct, 15722));
    assert!(backup_is_tainted("codex", managed, 15722));
}


#[test]
fn restore_skips_gateway_backups() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "current").unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260101-000000"),
        "model = \"orig\"\n",
    )
    .unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260201-000000"),
        "model_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
    )
    .unwrap();
    let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
    // 透明還原：返回值含配置路徑＋備份名＋摘要
    assert!(back.contains(&cfg.to_string_lossy().to_string()), "{back}");
    assert!(back.contains("還原自 config.toml.bak-20260101-000000"), "{back}");
    assert!(back.contains("model=orig"), "{back}");
    assert_eq!(
        std::fs::read_to_string(&cfg).unwrap(),
        "model = \"orig\"\n"
    );
    assert!(restore_backup_to(app_data, "cursor", &cfg).is_err());
}


#[test]
fn restore_allows_leftover_section_with_direct_url() {
    // 2026-09-24 codex 實例：接管殘留 tokengateway 段名、但 base_url 已改回
    // 直連上游——這是真正的接管前配置，必須能還原（舊段名規則會誤判卡死）。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "current").unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    let direct = "model = \"moonshotai/kimi-k3\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n\n[model_providers.tokengateway]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n";
    std::fs::write(bdir.join("config.toml.bak-20260101-000000"), direct).unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260201-000000"),
        "model_provider = \"tokengateway\"\n\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
    )
    .unwrap();
    let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
    assert!(back.contains("config.toml.bak-20260101-000000"), "{back}");
    assert!(back.contains("跳過 1 個含網關痕跡的備份"), "{back}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), direct);
}


#[test]
fn restore_ccswitch_state_warns_not_blocks() {
    // 接管前在用 cc-switch：允許還原（忠於接管前），但必須警告需啟動 cc-switch。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "current").unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260101-000000"),
        "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15721/v1\"\n",
    )
    .unwrap();
    let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
    assert!(back.contains("config.toml.bak-20260101-000000"), "{back}");
    assert!(back.contains("cc-switch"), "{back}");
}


#[test]
fn restore_migrates_clean_backup_to_immutable_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(
        &cfg,
        "model = \"m\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
    )
    .unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    let direct = "model = \"m\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
    std::fs::write(bdir.join("config.toml.bak-20260101-000000"), direct).unwrap();
    let back = restore_backup_to_port(app_data, "codex", &cfg, 15722).unwrap();
    assert!(back.contains("baseline-"), "{back}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), direct);
    assert!(!has_baseline(&bdir, "config.toml"));
}


#[test]
fn prune_keeps_clean_backups() {
    // 10 個接管態＋1 個最舊的乾淨備份：輪換必須淘汰接管態，乾淨備份存活。
    let dir = tempfile::tempdir().unwrap();
    let bdir = dir.path();
    std::fs::write(
        bdir.join("config.toml.bak-20260101-000000"),
        "model = \"orig\"\n",
    )
    .unwrap();
    for i in 2..=11 {
        std::fs::write(
            bdir.join(format!("config.toml.bak-202602{i:02}-000000")),
            "model_provider = \"tokengateway\"\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        )
        .unwrap();
    }
    prune_backups_keep_clean(bdir, "codex", "config.toml", 10, 15722).unwrap();
    let left: Vec<String> = std::fs::read_dir(bdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left.len(), 10, "{left:?}");
    assert!(left.contains(&"config.toml.bak-20260101-000000".to_string()), "{left:?}");
}


#[test]
fn restore_warns_direct_chat_upstream() {
    // 直連 Chat 上游卻走 responses 的備份：照樣還原，但必須警告 404 風險
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "current").unwrap();
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(
        bdir.join("config.toml.bak-20260301-000000"),
        "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n",
    )
    .unwrap();
    let back = restore_backup_to(app_data, "codex", &cfg).unwrap();
    assert!(back.contains("config.toml.bak-20260301-000000"), "{back}");
    assert!(back.contains("provider=custom"), "{back}");
    assert!(back.contains("404"), "{back}");
    assert_eq!(
        std::fs::read_to_string(&cfg).unwrap(),
        "model = \"m\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://integrate.api.nvidia.com/v1\"\nwire_api = \"responses\"\n"
    );
}


#[test]
fn backup_classifier_uses_configured_gateway_port() {
    let managed = "model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:18099/v1\"\n";
    assert!(!backup_is_tainted("codex", managed, 15722));
    assert!(backup_is_tainted("codex", managed, 18099));
}


#[test]
fn rotate_trims_timestamped_names() {
    // 輪換必須認 `{stem}.bak-{stamp}`（曾因 ends_with(".bak") 永不命中而無限堆積）。
    let dir = tempfile::tempdir().unwrap();
    for i in 1..=12 {
        std::fs::write(
            dir.path().join(format!("settings.json.bak-20260901-0000{i:02}")),
            "x",
        )
        .unwrap();
    }
    crate::fsutil::rotate_backups(dir.path(), "settings.json", 10).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    assert_eq!(names.len(), 10, "{names:?}");
    assert!(names.iter().all(|n| n.as_str() > "settings.json.bak-20260901-000002"), "{names:?}");
}


#[test]
fn backup_name_unique_within_same_second() {
    // 同秒重複備份不得覆蓋（否則丟失真正的接管前原設定）。
    let dir = tempfile::tempdir().unwrap();
    let base = "settings.json.bak-20260922-040000";
    let p1 = crate::fsutil::unique_backup_name(dir.path(), base);
    std::fs::write(&p1, "first").unwrap();
    let p2 = crate::fsutil::unique_backup_name(dir.path(), base);
    assert_ne!(p1, p2);
    std::fs::write(&p2, "second").unwrap();
    assert_eq!(std::fs::read_to_string(&p1).unwrap(), "first");
    // 後綴不破壞"最新優先"排序（仍排最前）。
    let mut v = vec![p1, p2];
    v.sort();
    v.reverse();
    assert!(v[0].to_string_lossy().ends_with("-2"));
}


#[test]
fn hermes_restore_brings_env_back() {
    // Hermes OFF 必須連帶還原 .env（tempdir 沙盒：cfg 父目錄即 .env 歸屬）。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.yaml");
    std::fs::write(&cfg, "current").unwrap();
    std::fs::write(&cfg.parent().unwrap().join(".env"), "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-now\n").unwrap();
    let bdir = app_data.join("backups").join("hermes");
    std::fs::create_dir_all(&bdir).unwrap();
    let clean_cfg = "model:\n  provider: flatkey\n";
    let clean_env = "# user env\nFLAT=1\n";
    std::fs::write(bdir.join("config.yaml.bak-20260101-000000"), clean_cfg).unwrap();
    std::fs::write(bdir.join(".env.bak-20260101-000000"), clean_env).unwrap();
    std::fs::write(
        bdir.join("config.yaml.bak-20260201-000000"),
        "providers:\n  tokengateway:\n    base_url: http://127.0.0.1:15722/v1\n",
    )
    .unwrap();
    std::fs::write(
        bdir.join(".env.bak-20260201-000000"),
        "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-then\n",
    )
    .unwrap();
    let msg = restore_backup_to(app_data, "hermes", &cfg).expect("restore");
    assert!(msg.contains(".env 已還原自 .env.bak-20260101-000000"), "{msg}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), clean_cfg);
    assert_eq!(
        std::fs::read_to_string(cfg.parent().unwrap().join(".env")).unwrap(),
        clean_env
    );
}


#[test]
fn hermes_env_all_tainted_keeps_current() {
    // .env 無乾淨備份時：保持現狀並注記，不猜不刪。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.yaml");
    std::fs::write(&cfg, "current").unwrap();
    let cur = "FLAT=1\nHERMES_CUSTOM_TOKENGATEWAY_API_KEY=up\n";
    std::fs::write(cfg.parent().unwrap().join(".env"), cur).unwrap();
    let bdir = app_data.join("backups").join("hermes");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(bdir.join("config.yaml.bak-20260101-000000"), "model:\n  provider: flatkey\n").unwrap();
    std::fs::write(
        bdir.join(".env.bak-20260101-000000"),
        "HERMES_CUSTOM_TOKENGATEWAY_API_KEY=up-old\n",
    )
    .unwrap();
    let msg = restore_backup_to(app_data, "hermes", &cfg).expect("restore");
    assert!(msg.contains(".env 無乾淨備份，保持現狀"), "{msg}");
    assert_eq!(
        std::fs::read_to_string(cfg.parent().unwrap().join(".env")).unwrap(),
        cur
    );
}


#[test]
fn restore_returns_numeric_localhost_original() {
    // T4（tempdir，零風險）：用戶原設定若指向數字 127.0.0.1 的本機服務
    //（如 http://127.0.0.1:11434），不得被當成網關殘留跳過。
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("settings.json");
    std::fs::write(&cfg, "current").unwrap();
    let bdir = app_data.join("backups").join("claude");
    std::fs::create_dir_all(&bdir).unwrap();
    let orig = r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:11434/v1"}}"#;
    std::fs::write(bdir.join("settings.json.bak-20260101-000000"), orig).unwrap();
    std::fs::write(
        bdir.join("settings.json.bak-20260201-000000"),
        r#"{"env": {"ANTHROPIC_BASE_URL": "http://127.0.0.1:15722", "ANTHROPIC_AUTH_TOKEN": "sk-local-x"}}"#,
    )
    .unwrap();
    let back = restore_backup_to(app_data, "claude", &cfg).expect("應還原數字 localhost 原設定");
    assert!(back.contains("settings.json.bak-20260101-000000"), "{back}");
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), orig);
}
