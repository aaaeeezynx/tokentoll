//! `prompt::sync` 的測試（從 `sync.rs` 搬出來，讓本體維持在 400 行以內）。


    use super::*;
    use crate::prompt::PromptInput;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn save(conn: &Connection, app: &str, name: &str, content: &str) -> PromptPreset {
        store::save(
            conn,
            &PromptInput {
                id: None,
                app: app.into(),
                name: name.into(),
                content: content.into(),
            },
        )
        .unwrap()
    }

    /// 回填：檔案被手改過時，切換前會把檔案內容存回**舊的**預設集。
    #[test]
    fn backfill_saves_manual_edits_into_the_previous_preset() {
        let (_d, conn) = db();
        let a = save(&conn, "codex", "A", "A 的內容");
        let b = save(&conn, "codex", "B", "B 的內容");
        store::activate(&conn, a.id).unwrap();

        // 模擬：使用者手改了檔案（不是透過我們）
        assert_eq!(
            simulate_backfill(&conn, "codex", "使用者手改過的內容"),
            "A",
            "手改的內容要存回啟用中的那個預設集"
        );
        assert_eq!(store::get(&conn, a.id).unwrap().content, "使用者手改過的內容");

        // 沒改動時不回填
        assert_eq!(simulate_backfill(&conn, "codex", "使用者手改過的內容"), "（無）");
        // 切到 B
        store::activate(&conn, b.id).unwrap();
        assert_eq!(store::get(&conn, a.id).unwrap().content, "使用者手改過的內容");
    }

    /// 測試輔助：只跑「回填」那一段（不碰真實檔案）。
    fn simulate_backfill(conn: &Connection, app: &str, live: &str) -> String {
        let Some(active) = store::active_of(conn, app).unwrap() else {
            return "（無啟用）".to_string();
        };
        if live != active.content {
            store::overwrite_content(conn, active.id, live).unwrap();
            active.name
        } else {
            "（無）".to_string()
        }
    }

    #[test]
    fn no_active_preset_means_no_backfill() {
        let (_d, conn) = db();
        save(&conn, "codex", "A", "內容");
        assert_eq!(simulate_backfill(&conn, "codex", "檔案內容"), "（無啟用）");
        // 預設集內容沒有被動到
        assert_eq!(store::list(&conn, "codex").unwrap()[0].content, "內容");
    }

    #[test]
    fn paths_match_the_documented_table() {
        let home = crate::tools::user_home().unwrap();
        assert_eq!(path_for("claude").unwrap(), home.join(".claude").join("CLAUDE.md"));
        assert_eq!(path_for("codex").unwrap(), home.join(".codex").join("AGENTS.md"));
        assert_eq!(
            path_for("opencode").unwrap(),
            home.join(".config").join("opencode").join("AGENTS.md")
        );
        assert!(path_for("gemini").is_err(), "不受管的工具要拒絕");
    }

    #[test]
    fn supported_apps_are_the_takeover_ones() {
        assert_eq!(supported_apps(), vec!["claude", "codex", "opencode"]);
    }

    // ── 真正寫檔的那一段（用暫存檔，不碰主目錄） ──

    /// 啟用中的內容會被寫進檔案；寫之前先備份（用「改了預設集內容」來驅動寫入）。
    #[test]
    fn sync_writes_the_active_preset_and_backs_up() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("AGENTS.md");

        let a = save(&conn, "codex", "A", "A 的內容");
        store::activate(&conn, a.id).unwrap();
        // ① 檔案不存在 → 建立它，沒有東西好備份
        let out = sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert!(out.wrote_file, "{out:?}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "A 的內容");
        assert!(out.backup.is_none(), "本來沒有檔案就沒東西好備份");

        // ② 使用者在 UI 改了預設集內容 → 存檔後由 write_active 寫進檔案（含備份）
        //    （測試一律走 *_at 版本：`write_active` 會解析真實主目錄，被安全鎖擋下）
        store::overwrite_content(&conn, a.id, "A 改過的內容").unwrap();
        let mut out3 = BackfillOutcome::default();
        write_active_at(&conn, &app_data, "codex", &file, &mut out3).unwrap();
        assert!(out3.wrote_file);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "A 改過的內容");
        let backup = out3.backup.expect("改寫前要有備份");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "A 的內容");

        // ③ 內容一致時再同步 → 不寫檔、不留新備份
        let out4 = sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert!(!out4.wrote_file);
        assert!(out4.backup.is_none());
    }

    /// **回填的完整流程**：手改檔案 → 切換預設集 → 手改的內容被存回舊預設集，
    /// 新預設集的內容進到檔案。
    #[test]
    fn switching_presets_backfills_manual_edits() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("AGENTS.md");

        let a = save(&conn, "codex", "A", "A 的內容");
        let b = save(&conn, "codex", "B", "B 的內容");
        store::activate(&conn, a.id).unwrap();
        sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "A 的內容");

        // 使用者用編輯器手改
        std::fs::write(&file, "我手改過的內容").unwrap();

        // 切到 B：先把「我手改過的內容」存回 A，再把 B 寫進檔案
        let out = activate_and_sync_at(&conn, &app_data, "codex", &file, b.id).unwrap();
        assert_eq!(out.backfilled_into, "A", "手改的內容要回填到 A");
        assert_eq!(out.backfilled_bytes, "我手改過的內容".len());
        assert_eq!(store::get(&conn, a.id).unwrap().content, "我手改過的內容");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "B 的內容");
        assert_eq!(store::get(&conn, b.id).unwrap().content, "B 的內容");
        // B 的內容沒有被回填污染
        assert!(!store::get(&conn, b.id).unwrap().content.contains("手改"));
    }

    /// 存檔路徑**不回填**：編輯啟用中的預設集後，使用者的編輯必須進到檔案。
    #[test]
    fn write_active_does_not_backfill_the_edit_away() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("AGENTS.md");
        let a = save(&conn, "codex", "A", "舊內容");
        store::activate(&conn, a.id).unwrap();
        sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();

        // 使用者在 UI 編輯啟用中的預設集
        store::overwrite_content(&conn, a.id, "使用者新寫的內容").unwrap();
        let mut out = BackfillOutcome::default();
        write_active_at(&conn, &app_data, "codex", &file, &mut out).unwrap();
        assert!(out.wrote_file);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "使用者新寫的內容");
        assert_eq!(store::get(&conn, a.id).unwrap().content, "使用者新寫的內容");
    }

    /// 首次啟動匯入：檔案有內容、資料庫一列都沒有 → 收成「現有內容」並啟用。
    #[test]
    fn first_run_imports_the_existing_file() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("AGENTS.md");
        std::fs::write(&file, "# 我原本的提示\n內容").unwrap();

        let out = sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert_eq!(out.imported, IMPORTED_NAME);
        let list = store::list(&conn, "codex").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, IMPORTED_NAME);
        assert!(list[0].active);
        assert_eq!(list[0].content, "# 我原本的提示\n內容");
        // 內容一致 → 不寫檔（原本就是這個內容）
        assert!(!out.wrote_file, "匯入後內容相同，不該重寫檔案");
        // 第二次同步不會再匯入一次
        let out2 = sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert!(out2.imported.is_empty());
        assert_eq!(store::list(&conn, "codex").unwrap().len(), 1);
    }

    /// 空檔案不該被匯入成一個空預設集。
    #[test]
    fn empty_file_is_not_imported() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("AGENTS.md");
        std::fs::write(&file, "   \n").unwrap();
        let out = sync_app_ex(&conn, &app_data, "codex", &file, true).unwrap();
        assert!(out.imported.is_empty());
        assert!(store::list(&conn, "codex").unwrap().is_empty());
    }

    /// 檔案不存在時首次匯入不動作；啟用預設集後會把檔案建立出來。
    #[test]
    fn missing_file_is_created_on_activation() {
        let (dir, conn) = db();
        let app_data = dir.path().join("data");
        let file = dir.path().join("sub").join("CLAUDE.md");
        let a = save(&conn, "claude", "A", "# 提示");
        store::activate(&conn, a.id).unwrap();
        let out = sync_app_ex(&conn, &app_data, "claude", &file, true).unwrap();
        assert!(out.wrote_file);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# 提示");
        assert!(out.backup.is_none(), "本來沒有檔案就沒東西好備份");
    }
