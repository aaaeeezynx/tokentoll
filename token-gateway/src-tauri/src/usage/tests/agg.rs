//! 聚合與匯出：summary / trend / trend_by_app / export_csv / hit_rate。
//!
//! 由 `usage/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    #[test]
    fn summary_and_filters() {
        let (_d, conn) = seed_db();
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        add_log(&conn, 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 2000, "codex", Some(1), "m1", 20, 10, 0.2, 500);
        add_log(&conn, 3000, "claude", Some(2), "m2", 30, 0, 0.3, 200);
        let s = summary(&conn, &f0).unwrap();
        assert_eq!((s.requests, s.ok_requests), (3, 2));
        assert_eq!((s.in_tok, s.out_tok), (60, 15));
        assert!((s.cost_usd - 0.6).abs() < 1e-9);
        assert!((s.success_rate - 2.0 / 3.0).abs() < 1e-9);
        // 時間過濾
        let s2 = summary(
            &conn,
            &UsageFilter {
                start_ts: 2000,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s2.requests, 2);
        // 應用過濾
        let s3 = summary(
            &conn,
            &UsageFilter {
                app: Some("claude".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((s3.requests, s3.in_tok), (1, 30));
    }

    #[test]
    fn trend_buckets_and_group_bys() {
        let (_d, conn) = seed_db();
        // ms 時間戳：day1=86_400_000, day2=172_800_000
        add_log(&conn, 86_400_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 172_800_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let t = trend(&conn, &f0, 86400).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].ts, 86_400_000);
        assert_eq!(t[1].tokens, 15);
        let bp = by_provider(&conn, &f0).unwrap();
        assert_eq!(bp.len(), 1);
        assert_eq!(bp[0].requests, 2);
        let bm = by_model(&conn, &f0).unwrap();
        assert_eq!(bm.len(), 1);
        assert_eq!(bm[0].model, "m1");
    }

    #[test]
    fn export_csv_respects_filter() {
        let (_d, conn) = seed_db();
        add_log(&conn, 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 2000, "claude", Some(1), "m,2", 20, 0, 0.2, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let csv = export_csv(&conn, &f0).unwrap();
        assert!(csv.starts_with("time,app,model,"));
        assert!(csv.contains("codex"));
        // 含逗號的模型名被引號包裹
        assert!(csv.contains("\"m,2\""));
        let csv2 = export_csv(
            &conn,
            &UsageFilter {
                app: Some("claude".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!csv2.contains("codex"));
    }

    #[test]
    fn trend_by_app_splits_apps_per_bucket() {
        let (_d, conn) = seed_db();
        add_log(&conn, 86_400_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        add_log(&conn, 86_400_000 + 2000, "claude", Some(1), "m1", 20, 0, 0.2, 200);
        add_log(&conn, 172_800_000 + 1000, "codex", Some(1), "m1", 10, 5, 0.1, 200);
        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let t = trend_by_app(&conn, &f0, 86400).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].ts, 86_400_000);
        assert_eq!(t[0].app, "claude");
        assert_eq!(t[0].tokens, 20);
        assert_eq!(t[1].app, "codex");
        assert_eq!(t[1].tokens, 15);
        assert_eq!(t[2].ts, 172_800_000);
    }

    #[test]
    fn hit_rate_is_zero_on_empty_denominator() {
        // 分母為 0 必須回 0.0，不可產生 NaN（NaN 會讓前端 fmtPct 顯示 "NaN%"）
        assert_eq!(hit_rate(0, 0), 0.0);
        assert!(!hit_rate(0, 0).is_nan());
        // 一般情形
        assert!((hit_rate(100, 300) - 0.75).abs() < 1e-9);
        // 全命中
        assert!((hit_rate(0, 50) - 1.0).abs() < 1e-9);
        // 全未命中
        assert!((hit_rate(50, 0) - 0.0).abs() < 1e-9);
    }
