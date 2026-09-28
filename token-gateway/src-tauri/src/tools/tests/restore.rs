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


/// 第一階段 E 的核心保證：`keep = 1` 時留下來的仍是**乾淨**那份。
///
/// 這條測試釘住 `prune_backups_keep_clean` 的兩個行為：
/// 1. 優先淘汰污染（接管態）備份，而不是照檔名順序砍；
/// 2. 乾淨備份是最後才被考慮淘汰的。
#[test]
fn prune_keep_one_keeps_the_clean_backup() {
    let dir = tempfile::tempdir().unwrap();
    let bdir = dir.path();
    // 最舊的是乾淨的，其後 5 份都是接管態
    std::fs::write(
        bdir.join("config.toml.bak-20260101-000000"),
        "model = \"orig\"\n",
    )
    .unwrap();
    for i in 2..=6 {
        std::fs::write(
            bdir.join(format!("config.toml.bak-202602{i:02}-000000")),
            "model_provider = \"tokengateway\"\n[model_providers.tokengateway]\nbase_url = \"http://127.0.0.1:15722/v1\"\n",
        )
        .unwrap();
    }
    prune_backups_keep_clean(bdir, "codex", "config.toml", BACKUP_KEEP, 15722).unwrap();
    let left: Vec<String> = std::fs::read_dir(bdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left, vec!["config.toml.bak-20260101-000000".to_string()], "{left:?}");
}


/// 第一階段 E 的完整情境：`BACKUP_KEEP = 1` 下「接管 → 再接管 → 還原」
/// 仍然要回到**最初的原始設定**。
///
/// 這裡逐步復刻 `apply_switch` 的備份序列（寫 baseline → 寫 bak → 輪換 → 寫入
/// 接管態），跑兩輪（第二輪沒有先還原，所以備份到的是接管態），最後走正式還原
/// 入口 `restore_backup_to_port`。
///
/// 註：原始設定必須帶可辨識的路由（`model_provider` ＋ `base_url`），否則
/// `migrate_gateway_baseline` 會視為「不是已知路由」而不採用——這是既有行為，
/// 不是本階段改的（順帶由這條測試記錄下來）。
#[test]
fn keep_one_still_restores_original_after_repeated_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    let stem = "config.toml";
    let orig = "model = \"orig\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
    let taken = "model = \"orig\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(&cfg, orig).unwrap();

    // 第一輪：內容乾淨 → 寫 baseline ＋ bak，輪換，然後寫入接管態
    write_baseline(&bdir, stem, orig).unwrap();
    std::fs::write(bdir.join("config.toml.bak-20260101-000000"), orig).unwrap();
    prune_backups_keep_clean(&bdir, "codex", stem, BACKUP_KEEP, 15722).unwrap();
    std::fs::write(&cfg, taken).unwrap();

    // 第二輪：沒有先還原，所以備份到的是接管態（污染備份）
    std::fs::write(bdir.join("config.toml.bak-20260201-000000"), taken).unwrap();
    prune_backups_keep_clean(&bdir, "codex", stem, BACKUP_KEEP, 15722).unwrap();

    // 只剩 1 份 bak，而且是乾淨的那份；baseline 仍在（輪換不碰它）
    let left: Vec<String> = std::fs::read_dir(&bdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("config.toml.bak-"))
        .collect();
    assert_eq!(left, vec!["config.toml.bak-20260101-000000".to_string()], "{left:?}");
    assert!(has_baseline(&bdir, stem), "baseline 不可被輪換刪掉");

    // 正式還原路徑（migrate=true）→ 回到最初的原始設定
    let back = restore_backup_to_port(app_data, "codex", &cfg, 15722).unwrap();
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), orig);
    assert!(back.contains("baseline-") || back.contains("bak-20260101"), "{back}");
}


/// 第一階段 E 的安全網：使用者現況就是「沒有 baseline、只有一串歷史備份」
/// （claude 10 份、opencode 10 份，全部沒有 baseline；hermes 原本也是如此，
/// 已於第一階段 F 隨 hermes 支援一起移除），
/// `keep = 1` 的輪換會把舊的砍掉——所以在砍之前必須先把**最乾淨的歷史備份**
/// 存成 baseline，否則最初的原始設定（可能只存在於最舊那份 bak 裡）會消失。
#[test]
fn keep_one_salvages_baseline_from_history_before_prune() {
    let dir = tempfile::tempdir().unwrap();
    let app_data = dir.path();
    let cfg = dir.path().join("config.toml");
    let orig = "model = \"orig\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"https://openrouter.ai/api/v1\"\n";
    let taken = "model = \"orig\"\nmodel_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"http://127.0.0.1:15722/v1\"\n";
    let bdir = app_data.join("backups").join("codex");
    std::fs::create_dir_all(&bdir).unwrap();
    std::fs::write(&cfg, taken).unwrap();
    // 最舊的那份是乾淨的原始設定，其後全是接管態；沒有 baseline
    std::fs::write(bdir.join("config.toml.bak-20260101-000000"), orig).unwrap();
    for i in 2..=5 {
        std::fs::write(
            bdir.join(format!("config.toml.bak-202602{i:02}-000000")),
            taken,
        )
        .unwrap();
    }
    assert!(!has_baseline(&bdir, "config.toml"));

    ensure_baseline_before_prune(app_data, "codex", &cfg, taken, 15722);
    assert!(has_baseline(&bdir, "config.toml"), "輪換前必須先保住接管前設定");

    // 再接管一次（寫入新 bak）並輪換到 1 份
    std::fs::write(bdir.join("config.toml.bak-20260301-000000"), taken).unwrap();
    prune_backups_keep_clean(&bdir, "codex", "config.toml", BACKUP_KEEP, 15722).unwrap();

    // 還原仍然回到最初的原始設定（走 baseline）
    let back = restore_backup_to_port(app_data, "codex", &cfg, 15722).unwrap();
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), orig);
    assert!(back.contains("baseline-"), "{back}");
}


/// 這個數字是**刻意選的**（第一階段 E，2026-09-28），不是隨手寫死的。
/// 要調大之前請先讀 `consts.rs::BACKUP_KEEP` 的說明，並確認上面兩條測試仍成立。
#[test]
fn backup_keep_is_one_by_design() {
    assert_eq!(BACKUP_KEEP, 1);
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
    let mut v = [p1, p2];
    v.sort();
    v.reverse();
    assert!(v[0].to_string_lossy().ends_with("-2"));
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
