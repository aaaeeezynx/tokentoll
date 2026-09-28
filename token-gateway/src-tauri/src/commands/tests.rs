    //! `commands.rs` 原本沒有任何測試。這裡只測**不需要 Tauri 執行期**的部分：
    //! 純函式與時間邊界。命令本身（`State<DbState>` 等）需要 AppHandle，
    //! 屬於整合測試範疇，不在這裡硬造。

    use super::*;

    // ---- range_start ----

    /// 每個 UI 提供的範圍都要有對應的起點，且落在合理區間。
    #[test]
    fn range_start_covers_every_ui_value() {
        let now = chrono::Local::now().timestamp_millis();
        // 前端 Usage.tsx 的 RANGES 清單
        for (range, days) in [
            ("7d", 7i64),
            ("30d", 30),
            ("90d", 90),
            ("180d", 180),
            ("365d", 365),
        ] {
            let start = range_start(range);
            let expect = now - days * 86400 * 1000;
            // 允許數秒誤差（now 在兩次呼叫之間會走動）
            assert!(
                (start - expect).abs() < 5000,
                "{range}：預期約 {expect}，得到 {start}"
            );
        }
    }

    /// `today` 必須是**本地時區的當天 0 點**，不能是 24 小時前。
    ///
    /// 這是容易寫錯的一條：用 `now - 86400000` 會變成「昨天此刻」，
    /// 使用者選「今天」卻看到跨兩天的資料。
    #[test]
    fn range_start_today_is_local_midnight() {
        let start = range_start("today");
        assert!(start > 0, "today 不該回退成 0");
        let dt = chrono::DateTime::from_timestamp_millis(start)
            .expect("合法時間戳")
            .with_timezone(&chrono::Local);
        use chrono::Timelike;
        assert_eq!(dt.hour(), 0, "應為 0 點：{dt}");
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.second(), 0);
        // 且必須落在今天（與現在同一個日曆日）
        assert_eq!(dt.date_naive(), chrono::Local::now().date_naive());
    }

    /// 未知範圍回傳 0（代表「不限起點」），不可 panic。
    #[test]
    fn range_start_unknown_is_zero() {
        assert_eq!(range_start("all"), 0);
        assert_eq!(range_start(""), 0);
        assert_eq!(range_start("Custom"), 0, "大小寫敏感，不該誤判為 custom");
    }

    // ---- range_bucket_span ----

    /// bucket 粒度只取三個可能值，且與跨度單調對應。
    #[test]
    fn range_bucket_span_picks_granularity() {
        let hour = 3600;
        let day = 86400;
        let week = 7 * 86400;
        let base = 1_700_000_000_000i64;

        // ≤ 2 天 → 小時
        assert_eq!(range_bucket_span(base, Some(base)), hour, "零跨度");
        assert_eq!(range_bucket_span(base, Some(base + 2 * day * 1000)), hour, "剛好 2 天");
        // > 2 天、≤ 186 天 → 日
        assert_eq!(
            range_bucket_span(base, Some(base + 2 * day * 1000 + 1)),
            day,
            "超過 2 天就該換成日"
        );
        assert_eq!(range_bucket_span(base, Some(base + 186 * day * 1000)), day, "剛好 186 天");
        // > 186 天 → 週
        assert_eq!(
            range_bucket_span(base, Some(base + 186 * day * 1000 + 1)),
            week,
            "超過 186 天就該換成週"
        );
    }

    /// `end_ts = None` 時以「現在」為終點（進行中的查詢）。
    #[test]
    fn range_bucket_span_uses_now_when_end_missing() {
        let now = chrono::Local::now().timestamp_millis();
        // 起點在 1 小時前、終點未指定 → 跨度約 1 小時 → 小時粒度
        assert_eq!(range_bucket_span(now - 3600 * 1000, None), 3600);
        // 起點在 100 天前 → 日粒度
        assert_eq!(range_bucket_span(now - 100 * 86400 * 1000, None), 86400);
    }

    /// 跨度上限：訊息與判斷式口徑一致（皆為 365 天）。
    ///
    /// 歷史：訊息寫「上限 365 天」但程式用的是 `> 366 * 86400 * 1000`，
    /// 造成 366 天可通過、367 天才被擋。2026-09-28 統一為 365，
    /// 讓「訊息說的」就是「程式做的」。
    #[test]
    fn resolve_filter_span_limit_is_365_days() {
        let now = chrono::Local::now().timestamp_millis();
        let day = 86400 * 1000;
        // 365 天整：必須通過（邊界含等於）
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 365 * day), Some(now)).is_ok(),
            "365 天應該允許"
        );
        // 366 天：必須擋下（先前會被放行，這是本次修掉的落差）
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 366 * day), Some(now)).is_err(),
            "366 天應該被擋（訊息說上限 365 天）"
        );
        // 367 天：必須擋下
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 367 * day), Some(now)).is_err(),
            "367 天應該被擋"
        );
        // 錯誤訊息本身也一併釘住，避免日後只改一邊
        let err = resolve_filter("custom", None, None, None, Some(now - 366 * day), Some(now))
            .expect_err("應為錯誤");
        assert!(err.contains("365 天"), "訊息必須寫 365 天：{err}");
    }

    /// 空字串的 app／model 要正規化成 None，否則 SQL 會用 `app = ''` 過濾掉全部。
    #[test]
    fn resolve_filter_normalizes_empty_strings() {
        let f = resolve_filter(
            "7d",
            Some(String::new()),
            None,
            Some(String::new()),
            None,
            None,
        )
        .expect("7d 應可用");
        assert_eq!(f.app, None, "空 app 應為 None");
        assert_eq!(f.model, None, "空 model 應為 None");
        // 有值時要保留
        let f = resolve_filter("7d", Some("codex".into()), Some(3), Some("m".into()), None, None)
            .expect("7d 應可用");
        assert_eq!(f.app.as_deref(), Some("codex"));
        assert_eq!(f.model.as_deref(), Some("m"));
        assert_eq!(f.provider_id, Some(3));
    }

    /// 未帶 end_ts 時，end 交給下游（`None`），不要硬塞「現在」——
    /// 硬塞會讓正在進行的查詢無法反映最新資料。
    #[test]
    fn resolve_filter_leaves_open_ended_end_as_none() {
        let now = chrono::Local::now().timestamp_millis();
        let f = resolve_filter("7d", None, None, None, None, None).expect("7d 應可用");
        assert_eq!(f.end_ts, None, "未指定結束時間應保持 None");
        assert!(f.start_ts < now);
        // 明確指定時才填
        let f = resolve_filter("7d", None, None, None, None, Some(now)).expect("應可用");
        assert_eq!(f.end_ts, Some(now));
    }
