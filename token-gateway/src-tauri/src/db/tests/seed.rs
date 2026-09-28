//! 種子與預設設定：只下發一次，且不覆蓋使用者已改的值。
//!
//! 由 `db/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    #[test]
    fn settings_defaults_seeded_without_overwriting_user_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            assert_eq!(
                get_setting(&c, "gateway_port").as_deref(),
                Some(DEFAULT_GATEWAY_PORT.to_string().as_str())
            );
            assert_eq!(get_setting(&c, "accent").as_deref(), Some("blue"));
            // 使用者改過之後重開不得被覆蓋
            set_setting(&c, "accent", "purple").unwrap();
            set_setting(&c, "gateway_port", "19999").unwrap();
        }
        let c = open(&path);
        assert_eq!(get_setting(&c, "accent").as_deref(), Some("purple"));
        assert_eq!(get_setting(&c, "gateway_port").as_deref(), Some("19999"));
    }

    #[test]
    fn seeds_are_not_revived_after_user_deletes_them() {
        // 種子只在 seed_version 落後時下發；使用者刪除後重開不應復活。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        {
            let c = open(&path);
            c.execute("DELETE FROM pricing", []).unwrap();
            c.execute("DELETE FROM providers", []).unwrap();
        }
        let c = open(&path);
        let p: i64 = c
            .query_row("SELECT COUNT(*) FROM pricing", [], |r| r.get(0))
            .unwrap();
        let v: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, 0, "使用者刪除的種子定價不應復活");
        assert_eq!(v, 0, "使用者刪除的種子渠道不應復活");
    }

    #[test]
    fn fresh_db_seeds_providers_and_pricing_once() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        let p: i64 = c
            .query_row("SELECT COUNT(*) FROM pricing WHERE source='seed'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(p as usize, SEED_PRICING.len());
        let v: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, SEED_PROVIDERS.len());
        // 種子渠道的 key 必須是空的（絕不內建金鑰）
        let nonempty: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM providers WHERE api_key <> ''",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nonempty, 0, "種子渠道不得內建任何金鑰");
    }
