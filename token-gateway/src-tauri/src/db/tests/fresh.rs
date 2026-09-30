//! 新庫形狀：schema 版本與核心表。
//!
//! 由 `db/tests/mod.rs` 拆分而來，內容與斷言一字未改。

use super::*;

    #[test]
    fn fresh_db_reaches_current_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        assert_eq!(max_version(&c), SCHEMA_VERSION as i64);
    }

    #[test]
    fn fresh_db_has_all_core_tables() {
        let dir = tempfile::tempdir().unwrap();
        let c = open(&dir.path().join("app.db"));
        for t in [
            "schema_version",
            "providers",
            "local_keys",
            "request_logs",
            "import_state",
            "pricing",
            "settings",
            "provider_pricing",
            "pricing_periods",
            "provider_models",
            "provider_stripped_fields",
            "provider_model_protocol",
            "provider_usage_query",
            "proxy_trace",
        ] {
            assert!(has_table(&c, t), "缺少表 {t}");
        }
    }
