//! 原 `history.rs` 的單元測試；除本檔頭的引入外，測試內容一字未改。

use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;

use super::commit::{commit_rows, gc_orphans, provider_name_map};
use super::parse::{parse_claude_file, parse_codex_file, parse_dsh_session, parse_opencode_db};

    use super::*;

    fn mem_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("m5.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn claude_parse_cache_split() {
        let text = concat!(
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-4-8\",\"usage\":{\"input_tokens\":10,\"output_tokens\":3,\"cache_read_input_tokens\":4,\"cache_creation_input_tokens\":1}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n",
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"},\"timestamp\":\"2026-09-10T01:02:04.000Z\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":2}},\"timestamp\":\"2026-09-10T01:03:00.000Z\"}\n",
            "not json\n",
        );
        let mut bad = 0;
        let rows = parse_claude_file(text, "f.jsonl", &mut bad);
        assert_eq!(rows.len(), 2);
        assert_eq!(bad, 1);
        assert_eq!(rows[0].model_raw, "claude-opus-4-8");
        assert_eq!(rows[0].in_tok, 10);
        assert_eq!(rows[0].cache_read, 4);
        assert_eq!(rows[0].cache_write, 1);
        assert_eq!(rows[1].model_raw, "");
        // 2026-09-10T01:02:03Z → 毫秒（與 chrono 對照）
        let expect = chrono::DateTime::parse_from_rfc3339("2026-09-10T01:02:03.000Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(rows[0].ts_ms, expect);
    }

    #[test]
    fn codex_parse_turn_join_and_fallback() {
        let text = concat!(
            "{\"timestamp\":\"2026-09-10T01:00:00.000Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"t1\",\"root_turn_id\":\"r1\",\"model\":\"deepseek-v4-flash\"}}\n",
            "{\"timestamp\":\"2026-09-10T01:00:01.000Z\",\"type\":\"token_usage_record\",\"payload\":{\"turn_id\":\"t1\",\"root_turn_id\":\"r1\",\"usage\":{\"input_tokens\":100,\"cached_input_tokens\":10,\"cache_write_input_tokens\":0,\"output_tokens\":20,\"reasoning_output_tokens\":5,\"total_tokens\":120}}}\n",
            "{\"timestamp\":\"2026-09-10T01:00:02.000Z\",\"type\":\"token_usage_record\",\"payload\":{\"turn_id\":\"tx\",\"root_turn_id\":\"rx\",\"usage\":{\"input_tokens\":7,\"output_tokens\":1}}}\n",
        );
        let mut bad = 0;
        let rows = parse_codex_file(text, "r.jsonl", &mut bad);
        assert_eq!(rows.len(), 2);
        assert_eq!(bad, 0);
        assert_eq!(rows[0].model_raw, "deepseek-v4-flash");
        assert_eq!(rows[0].in_tok, 100);
        assert_eq!(rows[0].cache_read, 10);
        // 未知 turn 回退到文件首模型
        assert_eq!(rows[1].model_raw, "deepseek-v4-flash");
    }

    #[test]
    fn dsh_parse_session_totals() {
        // DSH 的用量投影：prompt 拆成「未命中快取」與「命中快取」兩塊（互斥），
        // 所以總輸入要相加 —— 這樣才與 Codex／網關的 input_tokens（含快取）同口徑。
        let text = r#"{
          "version": 7,
          "record": {
            "identity": {"formatVersion": 3, "createdAt": 1790530758608, "cwd": "D:\\token counter"},
            "rows": {
              "tokenUsage": {"ver": 2, "seq": 9952, "val": {"totals": {
                 "uncachedInputTokens": 4447555, "outputTokens": 1191447,
                 "cacheReadTokens": 265470336, "cacheWriteTokens": 0}}},
              "modelSelection": {"ver": 1, "seq": 9, "val": {
                 "lastUsed": {"provider": "opencodego", "model": "deepseek-v4.1-flash"},
                 "pending": null}}
            }
          }
        }"#;
        let mut bad = 0;
        let rows = parse_dsh_session(text, "s.json", 1_790_750_000_000, &mut bad);
        assert_eq!(rows.len(), 1);
        assert_eq!(bad, 0);
        assert_eq!(rows[0].app, "dsh");
        assert_eq!(rows[0].model_raw, "deepseek-v4.1-flash");
        assert_eq!(rows[0].in_tok, 4447555 + 265470336, "輸入要含快取那一塊");
        assert_eq!(rows[0].out_tok, 1191447);
        assert_eq!(rows[0].cache_read, 265470336);
        assert_eq!(rows[0].cache_write, 0);
        assert_eq!(rows[0].ts_ms, 1_790_750_000_000, "時間由呼叫端（檔案 mtime）給");
        assert_eq!(rows[0].import_path, "s.json");
    }

    #[test]
    fn dsh_parse_skips_empty_and_bad() {
        let mut bad = 0;
        // 空會話（有投影但全是 0）不記。
        let empty = r#"{"record":{"rows":{"tokenUsage":{"val":{"totals":
            {"uncachedInputTokens":0,"outputTokens":0,"cacheReadTokens":0,"cacheWriteTokens":0}}}}}}"#;
        assert!(parse_dsh_session(empty, "e.json", 1, &mut bad).is_empty());
        assert_eq!(bad, 0);
        // 沒有 tokenUsage 投影 → 記 bad。
        let none = r#"{"record":{"rows":{"title":{"val":"x"}}}}"#;
        assert!(parse_dsh_session(none, "n.json", 1, &mut bad).is_empty());
        assert_eq!(bad, 1);
        // 不是 JSON → 記 bad。
        assert!(parse_dsh_session("not json", "b.json", 1, &mut bad).is_empty());
        assert_eq!(bad, 2);
        // 有量但沒有 modelSelection → 仍要記，用明確的佔位字串（不可靜默丟掉用量）。
        let nomodel = r#"{"record":{"rows":{"tokenUsage":{"val":{"totals":
            {"uncachedInputTokens":5,"outputTokens":2,"cacheReadTokens":0,"cacheWriteTokens":0}}}}}}"#;
        let rows = parse_dsh_session(nomodel, "x.json", 7, &mut bad);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_raw, "(DSH 未記錄模型)");
        assert_eq!(rows[0].in_tok, 5);
    }

    #[test]
    fn dsh_scan_counts_projections() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let sdir = home
            .join(".dsh")
            .join("storages")
            .join("session_projcache")
            .join("sessions");
        std::fs::create_dir_all(&sdir).unwrap();
        std::fs::write(sdir.join("a.json"), "{}").unwrap();
        std::fs::write(sdir.join("b.json"), "{}").unwrap();
        std::fs::write(sdir.join("ignore.txt"), "x").unwrap();
        let got = scan_history(home);
        let dsh = got.iter().find(|t| t.tool == "dsh").expect("要有 dsh");
        assert_eq!(dsh.files, 2, "只算 .json");
        assert_eq!(dsh.sessions, 2, "一個投影檔就是一場會話");
        assert!(dsh.bytes > 0);
    }

    #[test]
    fn import_idempotent_and_reimport_on_change() {
        let (_d, conn) = mem_db();
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("s.jsonl");
        std::fs::write(
            &f,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":100,\"output_tokens\":10}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n",
        )
        .unwrap();
        // 首輪
        let rows = parse_claude_file(
            &std::fs::read_to_string(&f).unwrap(),
            &f.to_string_lossy(),
            &mut 0,
        );
        let dp = vec![f.to_string_lossy().to_string()];
        let (n, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n, 1);
        let c1: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c1, 1);
        // 同指紋重跑 → 呼叫端會跳過；此處驗 commit 本身可重入（刪舊插新）
        let (n2, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n2, 1);
        let c2: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c2, 1, "重插不應翻倍");
        // 追加一行 → 行數變 2
        std::fs::write(
            &f,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":100,\"output_tokens\":10}},\"timestamp\":\"2026-09-10T01:02:03.000Z\"}\n{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-v4-flash\",\"usage\":{\"input_tokens\":50,\"output_tokens\":5}},\"timestamp\":\"2026-09-10T01:05:00.000Z\"}\n",
        )
        .unwrap();
        let rows = parse_claude_file(
            &std::fs::read_to_string(&f).unwrap(),
            &f.to_string_lossy(),
            &mut 0,
        );
        let (n3, _, _, _, _, _) = commit_rows(
            &conn,
            "claude",
            &f.to_string_lossy(),
            file_fp(&f),
            &rows,
            &dp,
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n3, 2);
        let c3: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c3, 2);
    }

    #[test]
    fn import_skips_gateway_dupes_and_prices() {
        let (_d, conn) = mem_db();
        // 先有一筆網關日誌
        conn.execute(
            "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, cost_usd, status, source)
             VALUES (1000000, 'codex', 'deepseek-v4-flash', 'deepseek-v4-flash', 100, 20, 0.01, 200, 'gateway')",
            [],
        )
        .unwrap();
        // 同 token 同模型同時間 → 判重跳過；另一筆不同 token → 寫入並按種子價計費
        let rows = vec![
            ParsedRow {
                app: "codex",
                model_raw: "deepseek-v4-flash".into(),
                provider_id: None,
                in_tok: 100,
                out_tok: 20,
                cache_read: 0,
                cache_write: 0,
                ts_ms: 1000050,
                import_path: "r.jsonl".into(),
            },
            ParsedRow {
                app: "codex",
                model_raw: "deepseek-v4-flash".into(),
                provider_id: None,
                in_tok: 200,
                out_tok: 30,
                cache_read: 0,
                cache_write: 0,
                ts_ms: 2000000,
                import_path: "r.jsonl".into(),
            },
        ];
        let (n, _, _, cost, dupes, _) = commit_rows(
            &conn,
            "codex",
            "r.jsonl",
            (1, 2),
            &rows,
            &["r.jsonl".to_string()],
            &mut HashMap::new(),
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(dupes, 1);
        // 200*0.14 + 30*0.28 = 36.4 / 1e6
        assert!((cost - 0.0000364).abs() < 1e-9, "cost={cost}");
        let src: String = conn
            .query_row(
                "SELECT source FROM request_logs WHERE import_path='r.jsonl'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src, "import");
    }

    #[test]
    fn opencode_session_parse_maps_provider() {
        let dir = tempfile::tempdir().unwrap();
        let oc = dir.path().join("oc.db");
        {
            let c = Connection::open(&oc).unwrap();
            c.execute_batch(
                "CREATE TABLE session (id TEXT, model TEXT, time_created INTEGER, time_updated INTEGER,
                 tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER,
                 tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL);",
            )
            .unwrap();
            c.execute(
                "INSERT INTO session VALUES ('s1', '{\"id\":\"deepseek-v4-flash\",\"providerID\":\"nvidia\"}', 1000, 2000, 50, 5, 2, 0, 0, 0.0)",
                [],
            )
            .unwrap();
            c.execute("INSERT INTO session VALUES ('s2', '', 1000, 2000, 9, 9, 0, 0, 0, 0.0)", [])
                .unwrap();
        }
        let (_dg, gw) = mem_db();
        // nvidia 種子渠道 id
        let pid: i64 = gw
            .query_row("SELECT id FROM providers WHERE name='nvidia'", [], |r| r.get(0))
            .unwrap();
        let oc_conn =
            Connection::open_with_flags(&oc, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let pmap = provider_name_map(&gw);
        let mut bad = 0;
        let rows = parse_opencode_db(&oc_conn, &pmap, &mut bad);
        assert_eq!(rows.len(), 1, "空模型應跳過");
        assert_eq!(rows[0].model_raw, "deepseek-v4-flash");
        assert_eq!(rows[0].provider_id, Some(pid));
        assert_eq!(rows[0].out_tok, 7, "reasoning 併入輸出");
        assert_eq!(rows[0].import_path, "opencode:s1");
    }

    #[test]
    fn gc_orphans_removes_stale_paths() {
        let (_d, conn) = mem_db();
        for p in ["keep.jsonl", "gone.jsonl"] {
            conn.execute(
                "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, source, import_path)
                 VALUES (1000, 'codex', 'm', 'm', 1, 1, 'import', ?1)",
                [p],
            )
            .unwrap();
        }
        // 網關行不受 GC 影響
        conn.execute(
            "INSERT INTO request_logs (ts, app, model_raw, model_norm, in_tok, out_tok, source, import_path)
             VALUES (1000, 'codex', 'm', 'm', 1, 1, 'gateway', 'gone.jsonl')",
            [],
        )
        .unwrap();
        let keep: std::collections::HashSet<String> = ["keep.jsonl".to_string()].into_iter().collect();
        let n = gc_orphans(&conn, "codex", &keep).unwrap();
        assert_eq!(n, 1);
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(c, 2);
    }

    #[test]
    fn scan_lists_tools() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".claude").join("projects").join("p")).unwrap();
        std::fs::write(
            home.path().join(".claude").join("projects").join("p").join("a.jsonl"),
            "{}\n",
        )
        .unwrap();
        let tools = scan_history(home.path());
        assert_eq!(tools.len(), 4);
        assert_eq!(tools[0].tool, "claude");
        assert_eq!(tools[0].files, 1);
        assert_eq!(tools[2].tool, "opencode");
        assert_eq!(tools[2].files, 0);
        // 順序固定：claude → codex → opencode → dsh（前端按這個順序列勾選框）。
        assert_eq!(tools[3].tool, "dsh");
        assert_eq!(tools[3].files, 0, "沒有投影目錄時回 0 而不是報錯");
        // 掃描清單與命令層白名單必須一致 —— 新增來源時最容易只改一半
        // （2026-09-30 的「未知工具：dsh」就是這樣發生的）。
        let ids: Vec<&str> = tools.iter().map(|t| t.tool.as_str()).collect();
        assert_eq!(ids, super::IMPORT_TOOLS.to_vec(), "白名單與掃描順序要一致");
    }
