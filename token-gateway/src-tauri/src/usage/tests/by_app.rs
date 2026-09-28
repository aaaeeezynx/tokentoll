//! 「工具」視角（by_app）的迴歸測試，以及渠道／模型視角的快取欄位。
//!
//! 由 `usage/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    // ── 分工具統計（「工具」視角）─────────────────────────────────────────

    #[test]
    fn by_app_row_matches_summary_for_that_app() {
        // 這是「工具」視角與「總覽」卡片數字一致的保證：
        // 每一列都必須等於 summary 加上該工具篩選後的結果。
        let (_d, conn) = seed_db();
        add_log_cache(&conn, 1000, "codex", "m1", 100, 20, 300, 0, 200);
        add_log_cache(&conn, 2000, "codex", "m1", 50, 10, 0, 0, 500);
        add_log_cache(&conn, 3000, "claude", "m2", 10, 5, 90, 0, 200);
        add_log_cache(&conn, 4000, "dsh", "m2", 7, 1, 0, 0, 200);

        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let rows = by_app(&conn, &f0).unwrap();
        assert_eq!(rows.len(), 3);

        let overall = summary(&conn, &f0).unwrap();
        // 加總必須等於整體
        assert_eq!(rows.iter().map(|r| r.requests).sum::<i64>(), overall.requests);
        assert_eq!(rows.iter().map(|r| r.tokens).sum::<i64>(), overall.total_tokens);
        assert_eq!(rows.iter().map(|r| r.in_tok).sum::<i64>(), overall.in_tok);
        assert_eq!(rows.iter().map(|r| r.cache_read).sum::<i64>(), overall.cache_read);

        for row in &rows {
            let f = UsageFilter {
                start_ts: 0,
                app: Some(row.app.clone()),
                ..Default::default()
            };
            let s = summary(&conn, &f).unwrap();
            assert_eq!(row.requests, s.requests, "{} 請求數不一致", row.app);
            assert_eq!(row.tokens, s.total_tokens, "{} tokens 不一致", row.app);
            assert_eq!(row.in_tok, s.in_tok, "{} in_tok 不一致", row.app);
            assert_eq!(row.cache_read, s.cache_read, "{} cache_read 不一致", row.app);
            assert!(
                (row.cache_hit_rate - s.cache_hit_rate).abs() < 1e-12,
                "{} 命中率不一致",
                row.app
            );
        }

        // 排序：tokens 由大到小（codex 420 > claude 105 > dsh 8）
        assert_eq!(rows[0].app, "codex");
        assert_eq!(rows[1].app, "claude");
        assert_eq!(rows[2].app, "dsh");
    }

    #[test]
    fn by_app_hit_rate_must_not_be_averaged_by_row() {
        // 迴歸測試：各工具命中率的「簡單平均」與正確的 token 加權值差距極大。
        // 前端若拿工具列自行平均，整體數字會嚴重錯誤 —— 本測試把這個陷阱釘住。
        let (_d, conn) = seed_db();
        // codex：大量 token，低命中（100 輸入 vs 100 快取讀 → 50%）
        add_log_cache(&conn, 1000, "codex", "m1", 100_000, 0, 100_000, 0, 200);
        // claude：極少 token，全命中（0 輸入 vs 1000 快取讀 → 100%）
        add_log_cache(&conn, 2000, "claude", "m2", 0, 0, 1_000, 0, 200);

        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let rows = by_app(&conn, &f0).unwrap();
        let overall = summary(&conn, &f0).unwrap();

        let naive = rows.iter().map(|r| r.cache_hit_rate).sum::<f64>() / rows.len() as f64;
        // 正確值＝(100_000+1_000) / (100_000 + 100_000+1_000) = 101_000/201_000 ≈ 0.5025
        assert!(
            (overall.cache_hit_rate - 101_000.0 / 201_000.0).abs() < 1e-9,
            "整體命中率算法錯誤：{}",
            overall.cache_hit_rate
        );
        // 印證兩者確實不同（若哪天有人「優化」成平均，這裡會失敗）
        assert!(
            (naive - overall.cache_hit_rate).abs() > 0.2,
            "簡單平均 {naive} 與正確值 {} 應有顯著差距",
            overall.cache_hit_rate
        );
    }

    #[test]
    fn by_app_applies_all_four_filters() {
        // 沿用 where_sql：四種篩選都必須生效，否則「工具」視角的數字會與其他頁不一致。
        let (_d, conn) = seed_db();
        add_log_cache(&conn, 1000, "codex", "m1", 100, 0, 0, 0, 200);
        add_log_cache(&conn, 2000, "codex", "m2", 200, 0, 0, 0, 200);
        add_log_cache(&conn, 3000, "claude", "m1", 400, 0, 0, 0, 200);

        let all = by_app(
            &conn,
            &UsageFilter {
                start_ts: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.len(), 2);

        // 依模型
        let only_m1 = by_app(
            &conn,
            &UsageFilter {
                start_ts: 0,
                model: Some("m1".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(only_m1.iter().map(|r| r.in_tok).sum::<i64>(), 500);

        // 依時間
        let early = by_app(
            &conn,
            &UsageFilter {
                start_ts: 0,
                end_ts: Some(1500),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(early.len(), 1);
        assert_eq!(early[0].app, "codex");
        assert_eq!(early[0].in_tok, 100);
    }

    #[test]
    fn provider_and_model_stats_expose_cache() {
        // 「來源」與「模型」視角新增的快取欄位必須真的有值。
        let (_d, conn) = seed_db();
        add_log_cache(&conn, 1000, "codex", "m1", 100, 20, 300, 5, 200);
        add_log_cache(&conn, 2000, "codex", "m1", 100, 20, 300, 5, 500);

        let f0 = UsageFilter {
            start_ts: 0,
            ..Default::default()
        };
        let ps = by_provider(&conn, &f0).unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].in_tok, 200);
        assert_eq!(ps[0].out_tok, 40);
        assert_eq!(ps[0].cache_read, 600);
        assert_eq!(ps[0].cache_write, 10);
        assert_eq!(ps[0].tokens, 850);
        assert_eq!((ps[0].requests, ps[0].ok_requests), (2, 1));
        assert!((ps[0].success_rate - 0.5).abs() < 1e-9);
        assert!((ps[0].cache_hit_rate - hit_rate(200, 600)).abs() < 1e-12);

        let ms = by_model(&conn, &f0).unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].in_tok, 200);
        assert_eq!(ms[0].cache_read, 600);
        assert_eq!(ms[0].cache_write, 10);
        assert_eq!(ms[0].tokens, 850);
        assert!((ms[0].cache_hit_rate - hit_rate(200, 600)).abs() < 1e-12);
    }

    #[test]
    fn by_app_unknown_bucket_is_preserved() {
        // `request_logs.app` 是 `TEXT NOT NULL DEFAULT ''`，所以「未知」有兩種來源：
        // 顯式的 'unknown'（infer_app 判定不出）與空字串（欄位預設值）。
        // 兩者都必須歸成同一個 'unknown' 桶，否則各工具加總會對不上整體數字。
        let (_d, conn) = seed_db();
        add_log_cache(&conn, 1000, "unknown", "m1", 11, 1, 0, 0, 200);
        add_log_cache(&conn, 2000, "", "m1", 13, 1, 0, 0, 200);
        let rows = by_app(
            &conn,
            &UsageFilter {
                start_ts: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1, "空字串與 'unknown' 必須合併為一列");
        assert_eq!(rows[0].app, "unknown");
        assert_eq!(rows[0].in_tok, 24);
        assert_eq!(rows[0].requests, 2);
    }
