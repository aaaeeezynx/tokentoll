//! 實機測試（需 `--ignored`，會寫入真實設定檔）
//!
//! 由 `tools.rs` 的測試模組拆分而來。測試內容與斷言一字未改；共用 helper
//! 由父模組 `tools::tests` 提供，經 `use super::*;` 取用。

use super::*;


#[test]
#[ignore = "live: writes real ~/.codex/config.toml"]
fn live_codex_takeover() {
    // 與命令層 switch_apply 相同的來源有效性守衛（已刪/停用來源拒絕寫入）。
    let home = dirs::home_dir().expect("home");
    let app_data = home
        .join("AppData")
        .join("Roaming")
        .join("com.tokencounter.gateway");
    let conn =
        rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
    let alive: bool = conn
        .query_row(
            "SELECT enabled FROM providers WHERE id=?1",
            rusqlite::params![LIVE_PID],
            |r| r.get::<_, i64>(0),
        )
        .map(|e| e == 1)
        .unwrap_or(false);
    assert!(
        alive,
        "守衛生效：來源 {LIVE_PID} 不存在或已停用，拒絕接管（config 未被改動）"
    );
    let catalog =
        crate::models::codex_catalog_generate(&conn, &home, &app_data, LIVE_PID, false)
            .expect("catalog");
    // 直連模式：注入該來源上游 Key（復刻命令層注入）。
    let api_key = if LIVE_DIRECT {
        conn.query_row(
            "SELECT api_key FROM providers WHERE id=?1",
            rusqlite::params![LIVE_PID],
            |r| r.get::<_, String>(0),
        )
        .expect("upstream key")
    } else {
        String::new()
    };
    drop(conn);
    let req = SwitchRequest {
        app: "codex".into(),
        base_url: "http://127.0.0.1:15722/v1".into(),
        api_key,
        model: LIVE_MODEL.into(),
        provider_id: Some(LIVE_PID),
        provider_format: None,
        reasoning: Some("medium".into()),
        context_window: None,
        gen_catalog: true,
        catalog_union: false,
        direct_upstream: LIVE_DIRECT,
        key_id: None,
        claude_map: None,
    };
    let res = apply_switch(
        &app_data,
        req,
        15722,
        Some(catalog.to_string_lossy().to_string()),
    )
    .expect("apply");
    println!("takeover ok: {res:?}");
}


#[test]
#[ignore = "live: writes real ~/.claude/settings.json (auto-restored)"]
fn live_claude_roundtrip() {
    let (_home, app_data) = live_dirs();
    let cfg = config_path_for("claude").expect("claude path");
    let snap = std::fs::read(&cfg).expect("read settings");
    assert!(
        !String::from_utf8_lossy(&snap).contains(GATEWAY_PROVIDER_ID)
            && !String::from_utf8_lossy(&snap).contains("127.0.0.1"),
        "前置：claude 當前應為乾淨原設定"
    );
    let before = live_bak_names(&app_data, "claude");
    let up = live_upstream(&app_data, LIVE_PID);
    // T1 ON
    let r1 = apply_switch(&app_data, live_req("claude", "http://127.0.0.1:15722", up.clone(), false), 15722, None).expect("on#1");
    println!("on#1: {r1:?}");
    let on1 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
    let on1_ok = on1.contains("127.0.0.1:15722") && on1.contains(&up) && serde_json::from_str::<serde_json::Value>(&on1).is_ok();
    // T2 ON→ON（秒級戳，需跨秒以得不同備份名）
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let r2 = apply_switch(&app_data, live_req("claude", "http://127.0.0.1:15722", up.clone(), false), 15722, None).expect("on#2");
    println!("on#2: {r2:?}");
    let on2 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
    let on2_ok = on2.contains("127.0.0.1:15722");
    // OFF（與開關同一恢復函數）
    let msg = restore_backup(&app_data, "claude").expect("restore");
    println!("off: {msg}");
    let off = std::fs::read(&cfg).expect("read");
    let active = detect_tools(15722).iter().find(|t| t.app == "claude").is_some_and(|t| t.gateway_active);
    // teardown → 斷言
    live_rm_new_baks(&app_data, "claude", &before);
    std::fs::write(&cfg, &snap).expect("teardown");
    assert!(on1_ok, "ON#1 未正確接管：{on1}");
    assert!(on2_ok, "ON#2 未正確接管：{on2}");
    assert!(!active, "OFF 後檢測仍顯示接管中");
    assert_eq!(off, snap, "T1/T2：OFF 未位元組還原原設定");
}


#[test]
#[ignore = "live: writes real opencode.json (auto-restored)"]
fn live_opencode_roundtrip() {
    // 本機現狀：30 個託管備份全部含網關痕跡（最早的也是接管態），無乾淨原設定。
    // 本測試記錄該卡住狀態（pre-restore 必錯），並驗證 ON 寫入正確、現場歸零。
    let (_home, app_data) = live_dirs();
    let cfg = config_path_for("opencode").expect("opencode path");
    let snap = std::fs::read(&cfg).expect("read opencode.json");
    let before = live_bak_names(&app_data, "opencode");
    let up = live_upstream(&app_data, LIVE_PID);
    // OFF（預期失敗：無乾淨備份可還原）
    let pre = restore_backup(&app_data, "opencode");
    println!("pre-off: {pre:?}");
    let stuck = pre.is_err();
    // ON（直連上游：api_key 為命令層注入的上游 Key，此處直接復刻注入後狀態）
    let r1 = apply_switch(&app_data, live_req("opencode", "http://127.0.0.1:15722/v1", up.clone(), true), 15722, None).expect("on#1");
    println!("on#1: {r1:?}");
    let on1 = String::from_utf8(std::fs::read(&cfg).expect("read")).expect("utf8");
    let v1: serde_json::Value = serde_json::from_str(&on1).expect("json");
    let on1_ok = v1["provider"]["tokengateway"]["options"]["baseURL"] == "http://127.0.0.1:15722/v1"
        && v1["provider"]["tokengateway"]["options"]["apiKey"] == serde_json::Value::String(up.clone());
    // teardown → 斷言（寫回測試前快照，機器狀態歸零）
    live_rm_new_baks(&app_data, "opencode", &before);
    std::fs::write(&cfg, &snap).expect("teardown");
    assert!(stuck, "預期本機 opencode 已無乾淨備份（pre-restore 應報錯），實際卻還原成功");
    assert!(on1_ok, "ON#1 未正確接管：{on1}");
}


#[test]
#[ignore = "live: temporarily relocates real backup dirs (auto-moved-back)"]
fn live_restore_no_backup() {
    // T3：無可用備份時，OFF 必須明確報錯且不碰配置文件。
    let (_home, app_data) = live_dirs();
    for app in ["claude", "opencode"] {
        let cfg = config_path_for(app).expect("cfg path");
        let snap = std::fs::read(&cfg).expect("read cfg");
        let dir = app_data.join("backups").join(app);
        let aside = app_data.join("backups").join(format!("{app}.t3aside"));
        let _ = std::fs::remove_dir_all(&aside);
        std::fs::rename(&dir, &aside).expect("move aside");
        let res = restore_backup(&app_data, app);
        let after = std::fs::read(&cfg).expect("read cfg");
        std::fs::rename(&aside, &dir).expect("move back");
        let listed = live_bak_names(&app_data, app);
        assert!(res.is_err(), "T3 {app}：無備份時應報錯，實際 Ok");
        assert_eq!(after, snap, "T3 {app}：無備份還原動了配置文件");
        assert!(!listed.is_empty(), "T3 {app}：備份目錄未移回");
        println!("T3 {app}: Err={}（文件未動，目錄已移回）", res.unwrap_err());
    }
}


#[test]
#[ignore = "live: queries npm registry (network)"]
fn live_tool_latest_print() {
    for l in tool_latest() {
        println!("{} latest={:?} up_to_date={:?}", l.app, l.latest, l.up_to_date);
    }
}


#[test]
#[ignore = "live: probes real installed tool versions (read-only)"]
fn live_tool_versions_shape() {
    // 契約：4 工具各一條；已安裝的有更新命令
    //（cursor／antigravity 第一階段 A、hermes 第一階段 F 都不在列）。
    let vs = tool_versions();
    assert_eq!(vs.len(), 4);
    for v in &vs {
        println!("{} installed={} version={:?} update={:?}", v.app, v.installed, v.version, v.update_label);
        if v.installed {
            assert!(!v.update_argv.is_empty(), "{} 已安裝卻無更新命令", v.app);
        }
    }
}


/// 真實接管（默认忽略，需显式 `-- --ignored live_opencode_direct` 執行）：
/// 對真實 ~/.config/opencode/opencode.json 做一次直連上游接管（默認 NIM 渠道 15），
/// 驗證注入的上游 Key 明文落盤（回归：曾靜默寫 {env:} 引用導致斷連）。
/// 前置：opencode 未運行；跑完後用 backups/opencode 下的新備份還原。
#[test]
#[ignore = "live: writes real ~/.config/opencode/opencode.json"]
fn live_opencode_direct() {
    let home = dirs::home_dir().expect("home");
    let app_data = home
        .join("AppData")
        .join("Roaming")
        .join("com.tokencounter.gateway");
    let conn =
        rusqlite::Connection::open(app_data.join("app.db")).expect("gateway app.db");
    let alive: bool = conn
        .query_row(
            "SELECT enabled FROM providers WHERE id=?1",
            rusqlite::params![LIVE_PID],
            |r| r.get::<_, i64>(0),
        )
        .map(|e| e == 1)
        .unwrap_or(false);
    assert!(alive, "守衛生效：來源 {LIVE_PID} 不存在或已停用，拒絕接管");
    // 直連模式：注入該來源上游 Key（復刻命令層注入）。
    let up: String = conn
        .query_row(
            "SELECT api_key FROM providers WHERE id=?1",
            rusqlite::params![LIVE_PID],
            |r| r.get(0),
        )
        .expect("upstream key");
    assert!(!up.trim().is_empty(), "來源 {LIVE_PID} 未填寫上游 Key");
    drop(conn);
    let req = SwitchRequest {
        app: "opencode".into(),
        base_url: "http://127.0.0.1:15722/v1".into(),
        api_key: up.clone(),
        model: LIVE_MODEL.into(),
        provider_id: Some(LIVE_PID),
        provider_format: None,
        reasoning: None,
        context_window: None,
        gen_catalog: false,
        catalog_union: false,
        direct_upstream: true,
        key_id: None,
        claude_map: None,
    };
    let res = apply_switch(&app_data, req, 15722, None).expect("apply");
    println!("takeover ok: {res:?}");
    let text = std::fs::read_to_string(
        home.join(".config").join("opencode").join("opencode.json"),
    )
    .expect("opencode.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        v["provider"]["tokentoll"]["options"]["baseURL"],
        "http://127.0.0.1:15722/v1",
        "直連模式 baseURL 仍是網關：{text}"
    );
    assert_eq!(
        v["provider"]["tokentoll"]["options"]["apiKey"], up,
        "上游 Key 必須明文落盤（不得是 env 引用）：{text}"
    );
}
