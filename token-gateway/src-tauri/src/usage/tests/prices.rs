//! 定價試算、訂閱月費與全域定價 CRUD。
//!
//! 由 `usage/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    #[test]
    fn price_quote_math_and_source() {
        let (_d, conn) = seed_db();
        let (pid, pname): (i64, String) = conn
            .query_row("SELECT id, name FROM providers LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        pp_upsert(
            &conn,
            &ProviderPricingInput {
                provider_id: pid,
                model_norm: "m1".into(),
                mode: "usage".into(),
                in_pm: 2.0,
                out_pm: 8.0,
                cache_read_pm: 0.2,
                cache_create_pm: 0.0,
                sub_fee_usd: 0.0,
                sub_included_tokens: -1,
            },
        )
        .unwrap();
        let q = price_quote(&conn, Some(pid), "M1", 1_000_000, 500_000, 0, 0).unwrap();
        assert!((q.cost_usd - 6.0).abs() < 1e-9);
        assert_eq!(q.source, format!("{pname}・模型"));
        assert_eq!(q.mode, "usage");
        // 訂閱行：回傳月費/額度/已用
        pp_upsert(
            &conn,
            &ProviderPricingInput {
                provider_id: pid,
                model_norm: "msub".into(),
                mode: "subscription".into(),
                in_pm: 2.0,
                out_pm: 8.0,
                cache_read_pm: 0.0,
                cache_create_pm: 0.0,
                sub_fee_usd: 20.0,
                sub_included_tokens: 1000,
            },
        )
        .unwrap();
        let qs = price_quote(&conn, Some(pid), "msub", 100, 0, 0, 0).unwrap();
        assert_eq!(qs.mode, "subscription");
        assert!((qs.sub_fee_usd - 20.0).abs() < 1e-9);
        assert_eq!(qs.sub_included_tokens, 1000);
        assert_eq!(qs.cost_usd, 20.0);
        assert_eq!(qs.sub_over_tokens, 0);
        // 本月已用 900，場景 1000 → 超 900，按基礎輸入價 $2/M
        add_log(
            &conn,
            crate::fsutil::now_ms(),
            "codex",
            Some(pid),
            "msub",
            900,
            0,
            0.0,
            200,
        );
        let qo = price_quote(&conn, Some(pid), "msub", 1000, 0, 0, 0).unwrap();
        assert_eq!(qo.sub_used_this_month, 900);
        assert_eq!(qo.sub_over_tokens, 900);
        assert!((qo.cost_usd - (20.0 + 900.0 / 1e6 * 2.0)).abs() < 1e-9);
        let fees = subscription_fees(&conn).unwrap();
        assert!(fees.fees.iter().any(|f| f.provider_id == pid && f.model_norm == "msub"));
        assert!(fees.total_fee >= 20.0);
        // 全域回退
        let q2 = price_quote(&conn, Some(pid), "nope", 1_000_000, 0, 0, 0).unwrap();
        assert_eq!(q2.source, "未定價（記 0）");
        assert_eq!(q2.cost_usd, 0.0);
    }

    #[test]
    fn pricing_crud() {
        let (_d, conn) = seed_db();
        pricing_upsert(
            &conn,
            &PricingInput {
                model_norm: "My-Model".into(),
                in_pm: 1.0,
                out_pm: 2.0,
                cache_read_pm: 0.1,
                cache_create_pm: 0.2,
            },
        )
        .unwrap();
        let all = pricing_list(&conn).unwrap();
        assert!(all.iter().any(|p| p.model_norm == "my-model" && p.source == "custom"));
        assert!(pricing_upsert(
            &conn,
            &PricingInput {
                model_norm: "bad".into(),
                in_pm: -1.0,
                out_pm: 0.0,
                cache_read_pm: 0.0,
                cache_create_pm: 0.0,
            },
        )
        .is_err());
        pricing_delete(&conn, "my-model").unwrap();
        assert!(pricing_delete(&conn, "my-model").is_err());
    }
