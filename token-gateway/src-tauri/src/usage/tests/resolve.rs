//! 費率決議：優先級、日期尾綴回退、時段（tou）、訂閱額度。
//!
//! 由 `usage/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    #[test]
    fn resolve_filter_validates_custom() {
        use crate::commands::resolve_filter;
        assert!(resolve_filter("custom", None, None, None, None, None).is_err());
        assert!(resolve_filter("custom", None, None, None, Some(2000), Some(1000)).is_err());
        let now = crate::fsutil::now_ms();
        assert!(
            resolve_filter("custom", None, None, None, Some(now - 400 * 86400 * 1000), Some(now))
                .is_err()
        );
        assert!(
            resolve_filter("custom", None, None, None, Some(now + 3600 * 1000), Some(now + 7200 * 1000))
                .is_err()
        );
        let f =
            resolve_filter("custom", None, None, None, Some(now - 1000), Some(now)).unwrap();
        assert_eq!(f.end_ts, Some(now));
    }

    #[test]
    fn resolve_priority_and_modes() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "flatkey");
        // 全域回退：無渠道定價時用全域 seed
        let r = resolve_rates(&conn, Some(pid), "deepseek-v4-flash", ts_at(10, 0));
        assert!((r.in_pm - 0.14).abs() < 1e-9);
        // 渠道 * 通配優先於全域
        pp_upsert(&conn, &pp(pid, "*", "usage")).unwrap();
        let r = resolve_rates(&conn, Some(pid), "whatever-model", ts_at(10, 0));
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // 精確優先於通配
        let mut exact = pp(pid, "m1", "usage");
        exact.in_pm = 5.0;
        pp_upsert(&conn, &exact).unwrap();
        let r = resolve_rates(&conn, Some(pid), "m1", ts_at(10, 0));
        assert!((r.in_pm - 5.0).abs() < 1e-9);
        // 未知模型無定價 → 0
        let r = resolve_rates(&conn, None, "nope", ts_at(10, 0));
        assert_eq!(r, RateSet::zero());
    }

    #[test]
    fn resolve_date_tail_fallback() {
        let (_d, conn) = seed_db();
        // deepseek-v4-pro 有種子價；-0813 尾綴回退命中父級
        let r = resolve_rates(&conn, None, "deepseek-v4-pro-0813", 0);
        assert!((r.in_pm - 1.74).abs() < 1e-9);
        assert!((r.out_pm - 3.48).abs() < 1e-9);
        // 渠道鏈路同樣回退（無渠道行 → 全域父級）
        let pid = provider_id(&conn, "flatkey");
        let r = resolve_rates(&conn, Some(pid), "deepseek-v4-pro-0813", 0);
        assert!((r.in_pm - 1.74).abs() < 1e-9);
        // kimi-k2-0905 父級無價 → 保持 0（行為不變）
        assert_eq!(
            resolve_rates(&conn, None, "kimi-k2-0905", 0),
            RateSet::zero()
        );
        // 非日期尾綴不受影響
        assert!(resolve_rates(&conn, None, "gpt-5", 0).in_pm > 0.0);
    }

    #[test]
    fn resolve_tou_multi_periods() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "zen");
        pp_upsert(&conn, &pp(pid, "m-tou", "tou")).unwrap();
        // deepseek 式多時段：早峰 08:00-12:00、晚峰 18:00-22:00、深夜跨夜 22:00-06:00
        pp_period_add(&conn, &period(pid, "m-tou", "早峰", "08:00", "12:00")).unwrap();
        let mut evening = period(pid, "m-tou", "晚峰", "18:00", "22:00");
        evening.in_pm = 3.0;
        pp_period_add(&conn, &evening).unwrap();
        let night = period(pid, "m-tou", "深夜", "22:00", "06:00");
        pp_period_add(&conn, &night).unwrap();
        // 各時段命中各自費率
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(10, 0));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(19, 0));
        assert!((r.in_pm - 3.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(23, 30));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(5, 59));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        // 空隙（12:00-18:00）回退基礎價
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(14, 0));
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // `*` 時段作為其他模型的回退（自有時段優先）
        let mut promo = period(pid, "*", "全天促銷", "00:00", "23:59");
        promo.in_pm = 9.0;
        pp_period_add(&conn, &promo).unwrap();
        let r = resolve_rates(&conn, Some(pid), "other-model", ts_at(3, 0));
        assert!((r.in_pm - 9.0).abs() < 1e-9);
        // 自有時段優先於 *（同為 03:00，m-tou 有深夜時段 2.0）
        let r = resolve_rates(&conn, Some(pid), "m-tou", ts_at(3, 0));
        assert!((r.in_pm - 2.0).abs() < 1e-9);
        // 時段 CRUD 與校驗
        let list = pp_periods(&conn, pid, "m-tou").unwrap();
        assert_eq!(list.len(), 3);
        let id = list[0].id;
        let mut upd = period(pid, "m-tou", "早峰改", "08:00", "12:00");
        upd.ord = 5;
        pp_period_update(&conn, id, &upd).unwrap();
        assert!(pp_period_update(&conn, 999999, &upd).is_err());
        let mut bad = period(pid, "m-tou", "壞", "25:00", "12:00");
        bad.start = "25:00".into();
        assert!(pp_period_add(&conn, &bad).is_err());
        pp_period_delete(&conn, id).unwrap();
        assert!(pp_period_delete(&conn, id).is_err());
        assert_eq!(pp_periods(&conn, pid, "m-tou").unwrap().len(), 2);
    }

    #[test]
    fn resolve_subscription_included_then_overage() {
        let (_d, conn) = seed_db();
        let pid = provider_id(&conn, "nvidia");
        pp_upsert(&conn, &pp(pid, "m-sub", "subscription")).unwrap();
        let now = crate::fsutil::now_ms();
        // 期內 400 < 1000 → 免費
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,'codex',?2,'m-sub','m-sub',300,100,0,10,200,0)",
            rusqlite::params![now, pid],
        )
        .unwrap();
        let r = resolve_rates(&conn, Some(pid), "m-sub", now);
        assert_eq!(r, RateSet::zero());
        // 再用 700 → 累計 1100 > 1000 → 超額按基礎價
        conn.execute(
            "INSERT INTO request_logs (ts, app, provider_id, model_raw, model_norm, in_tok, out_tok, cost_usd, latency_ms, status, is_stream)
             VALUES (?1,'codex',?2,'m-sub','m-sub',500,200,0,10,200,0)",
            rusqlite::params![now, pid],
        )
        .unwrap();
        let r = resolve_rates(&conn, Some(pid), "m-sub", now);
        assert!((r.in_pm - 1.0).abs() < 1e-9);
        // 通配訂閱按渠道匯總
        pp_upsert(&conn, &pp(pid, "*", "subscription")).unwrap();
        let list = pp_list(&conn, Some(pid), now).unwrap();
        let wild = list.iter().find(|p| p.model_norm == "*").unwrap();
        assert_eq!(wild.used_this_month, 1100);
        // 非法輸入被拒
        let mut bad = pp(pid, "x", "nope");
        bad.mode = "nope".into();
        assert!(pp_upsert(&conn, &bad).is_err());
        let mut bad2 = pp(pid, "", "usage");
        bad2.model_norm = "".into();
        assert!(pp_upsert(&conn, &bad2).is_err());
        pp_delete(&conn, pid, "m-sub").unwrap();
        assert!(pp_delete(&conn, pid, "m-sub").is_err());
    }
