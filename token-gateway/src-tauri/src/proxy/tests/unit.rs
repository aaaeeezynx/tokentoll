//! 單元測試：路徑／模型／用量解析
//!
//! 由 `proxy/tests.rs` 拆分而來（原檔 2,025 行）。測試**內容與斷言一字
//! 未改**，只搬位置；對外仍經由 `proxy::tests::` 這個路徑被呼叫。

    // 前置由父模組 `proxy::tests` 的 `pub(super) use` 再匯出提供（含 `SseUsage`
    // 與 `E_*` 常數）；`use super::*;` 會把它們一併帶進來。這裡**不可**再寫
    // `use super::matrix::…` —— 在子模組裡 `super` 是 `proxy::tests`，而那裡
    // 的 `matrix` 指的是測試子模組本身。
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn stream_options_only_for_chat() {
        assert!(should_inject_usage(false, "/v1/chat/completions"));
        assert!(should_inject_usage(true, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1/responses"));
        assert!(!should_inject_usage(false, "/v1/messages"));
        assert!(!should_inject_usage(false, "/v1beta/models/x:generateContent"));
    }


    #[test]
    fn unknown_field_parse_and_strip() {
        assert_eq!(
            parse_unknown_fields(r#"json: unknown field "verbosity" Request id: 1"#),
            vec!["verbosity"]
        );
        // JSON 轉義形態（真實回包樣子）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"json: unknown field \"verbosity\" Request id: 1"}"#),
            vec!["verbosity"]
        );
        assert!(parse_unknown_fields("plain 400").is_empty());
        assert!(parse_unknown_fields("unknown field \"\"").is_empty());
        let body = br#"{"model":"m","verbosity":"low","stream":true}"#;
        let stripped = strip_json_field(body, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("verbosity").is_none());
        assert_eq!(v["model"], "m");
        assert!(strip_json_field(body, "nope").is_none());
        assert!(strip_json_field(b"not json", "verbosity").is_none());
        // Codex 式：verbosity 藏在 text 裡，清空後 text 一併移除
        let nested = br#"{"model":"m","text":{"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("text").is_none());
        let nested2 = br#"{"model":"m","text":{"format":{"type":"text"},"verbosity":"low"}}"#;
        let stripped = strip_json_field(nested2, "verbosity").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v["text"]["format"]["type"] == "text");
    }


    #[test]
    fn openrouter_unsupported_parameter_parse() {
        // OpenRouter 真實口徑（2026-09-24 codex 實例：prompt_cache_key 被拒）
        assert_eq!(
            parse_unknown_fields(r#"{"message":"Validation: Unsupported parameter(s): 'prompt_cache_key'","type":"Bad Request","code":400}"#),
            vec!["prompt_cache_key"]
        );
        // 多個一次全取出
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): 'a', 'b-c_d'"),
            vec!["a", "b-c_d"]
        );
        assert_eq!(
            parse_unknown_fields("Validation: Unsupported parameter(s): `prompt_cache_key`"),
            vec!["prompt_cache_key"]
        );
        assert_eq!(
            parse_unknown_fields("Unsupported parameter(s): \"a\", 'b'"),
            vec!["a", "b"]
        );
        assert!(parse_unknown_fields("Unsupported parameter(s): none").is_empty());
        // 剝離後重發體：prompt_cache_key 移除，其餘不動
        let body = br#"{"model":"m","prompt_cache_key":"sess-1","messages":[]}"#;
        let stripped = strip_json_field(body, "prompt_cache_key").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&stripped).unwrap();
        assert!(v.get("prompt_cache_key").is_none());
        assert_eq!(v["model"], "m");
    }


    #[test]
    fn join_upstream_dedups_v1() {
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/chat/completions"),
            "https://router.flatkey.ai/v1/chat/completions"
        );
        assert_eq!(
            join_upstream("https://router.flatkey.ai/v1", "/v1/messages"),
            "https://router.flatkey.ai/v1/messages"
        );
        assert_eq!(
            join_upstream("https://host", "/v1/models"),
            "https://host/v1/models"
        );
    }


    #[test]
    fn normalize_model_rules() {
        assert_eq!(normalize_model("anthropic/claude-opus-4-8"), "claude-opus-4-8");
        assert_eq!(normalize_model("GPT-5.2-codex@low"), "gpt-5.2-codex-low");
        // -v1 / 日期 / 渠道尾綴剝除（短 ID 兜底定價）
        assert_eq!(
            normalize_model("global.anthropic.claude-opus-4-8-v1:0"),
            "global.anthropic.claude-opus-4-8"
        );
        assert_eq!(
            normalize_model("deepseek-v4-flash-ga-260731"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            normalize_model("gpt-4o-mini-2024-07-18"),
            "gpt-4o-mini"
        );
        assert_eq!(
            normalize_model("moonshotai/kimi-k2-0905:exa"),
            "kimi-k2-0905"
        );
        // 語義編號不受影響
        assert_eq!(normalize_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_model("llama-3"), "llama-3");
        assert_eq!(normalize_model("kimi-k2-turbo"), "kimi-k2-turbo");
        assert_eq!(normalize_model("kimi-k2-0905"), "kimi-k2-0905");
        assert_eq!(normalize_model("  kimi-k2-0905[1m] "), "kimi-k2-0905");
    }


    #[test]
    fn extract_openai_chat_usage() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"model":"gpt-5","usage":{"prompt_tokens":10,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":3}}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 5, 3));
        assert_eq!(u.model.as_deref(), Some("gpt-5"));
    }


    #[test]
    fn extract_anthropic_stream_usage() {
        let start: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_start","message":{"model":"claude-opus-4-8","usage":{"input_tokens":7,"cache_read_input_tokens":2}}}"#,
        )
        .unwrap();
        let delta: serde_json::Value = serde_json::from_str(
            r#"{"type":"content_block_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        // message_delta 形狀
        let delta2: serde_json::Value = serde_json::from_str(
            r#"{"type":"message_delta","usage":{"output_tokens":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&start, &mut u);
        extract_usage(&delta, &mut u);
        extract_usage(&delta2, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (7, 4, 2));
    }


    #[test]
    fn sse_accumulates_across_chunks() {
        let mut acc = SseAcc::new();
        acc.feed(b"data: {\"model\":\"gpt-5\",\"choices\":[]}\n\ndata: ");
        acc.feed(b"{\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n");
        assert_eq!((acc.usage.in_tok, acc.usage.out_tok), (10, 5));
        assert_eq!(acc.usage.model.as_deref(), Some("gpt-5"));
    }


    #[test]
    fn infer_app_rules() {
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static("codex-cli/1.0"));
        assert_eq!(infer_app(&h), "codex");
        h.insert("x-tg-app", HeaderValue::from_static("hermes"));
        assert_eq!(infer_app(&h), "hermes");
        h.insert("x-tg-app", HeaderValue::from_static("nope"));
        assert_eq!(infer_app(&h), "codex", "非法聲明回退到 UA");
        h.insert("x-tg-app", HeaderValue::from_static("cursor"));
        assert_eq!(infer_app(&h), "cursor");
        let mut h2 = HeaderMap::new();
        h2.insert(
            "user-agent",
            HeaderValue::from_static("antigravity/1.0"),
        );
        assert_eq!(infer_app(&h2), "antigravity");
    }

    /// DSH 的**真實** User-Agent 必須歸為 `dsh`，不可被別人攔截。
    ///
    /// 這個字串取自 DSH 的 `dsh-llm` 套件原始碼：`attributionHeaders()` 會送出
    /// `product/version (+url)`，其中 product 為 `deepseek-harness`、url 指向
    /// `github.com/deepseek-ai/deepseek-harness`。
    #[test]
    fn infer_app_dsh_real_user_agent() {
        let mut h = HeaderMap::new();
        h.insert(
            "user-agent",
            HeaderValue::from_static(
                "deepseek-harness/0.1.5-rc.3 (+https://github.com/deepseek-ai/deepseek-harness)",
            ),
        );
        assert_eq!(infer_app(&h), "dsh");
    }

    /// **這是實際回報的 bug。**
    ///
    /// 舊寫法按固定順序（codex 在最前面）找第一個 `contains`，所以只要 UA
    /// 同時含 `codex` 與 DSH 的識別字，就會被記成 Codex —— 而且因為回傳的是
    /// 合法的 app 名稱，畫面上完全看不出歸錯了。
    ///
    /// DSH 本身會載入 `dsh-hooks-codex` 等套件；一旦客戶端把這些名字併進 UA，
    /// 就會踩到。修法後以「最具體的字串」優先，`deepseek-harness` 勝過 `codex`。
    #[test]
    fn infer_app_dsh_wins_over_codex_when_both_present() {
        for ua in [
            // DSH 的 UA 後面被接上 codex 相容標註
            "deepseek-harness/0.1.5-rc.3 (+https://github.com/deepseek-ai/deepseek-harness) codex",
            "codex_cli_rs/0.20.0 (deepseek-harness)",
            // 使用者回報的實際形狀：打網關的其實是 DSH，但帶著 codex 字樣
            "codex/1.0 deepseek-harness/0.1.5",
            // opencode 同理：dsh-opencode-session 若洩漏進 UA
            "deepseek-harness/0.1.5 opencode",
        ] {
            let mut h = HeaderMap::new();
            h.insert("user-agent", HeaderValue::from_str(ua).unwrap());
            assert_eq!(
                infer_app(&h),
                "dsh",
                "UA 同時含 DSH 與他人字樣時應歸 dsh：{ua}"
            );
        }
    }

    /// 純 Codex 的 UA 不受影響（修法不可為了修 DSH 而弄壞 Codex）。
    #[test]
    fn infer_app_pure_codex_still_codex() {
        for ua in [
            "codex_cli_rs/0.20.0",
            "codex/1.0",
            "OpenAI/Codex 1.0",
        ] {
            let mut h = HeaderMap::new();
            h.insert("user-agent", HeaderValue::from_str(ua).unwrap());
            assert_eq!(infer_app(&h), "codex", "純 Codex UA 應維持 codex：{ua}");
        }
    }

    /// `X-TG-App` 顯式聲明永遠優先於任何 UA 猜測。
    ///
    /// 這是唯一能 100% 確定歸屬的機制，也是未來若再有第三方客戶端
    /// 撞名時的正解 —— 不必再來改這張表。
    #[test]
    fn infer_app_explicit_header_beats_ambiguous_ua() {
        let mut h = HeaderMap::new();
        h.insert(
            "user-agent",
            HeaderValue::from_static("codex/1.0 deepseek-harness/0.1.5"),
        );
        h.insert("x-tg-app", HeaderValue::from_static("dsh"));
        assert_eq!(infer_app(&h), "dsh");
        // 反過來也一樣：聲明 codex 就以 codex 為準
        h.insert("x-tg-app", HeaderValue::from_static("codex"));
        assert_eq!(infer_app(&h), "codex");
    }

    /// 沒有任何線索時回 `unknown`，不可亂猜成某個工具。
    #[test]
    fn infer_app_unknown_when_no_signal() {
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static("python-requests/2.31"));
        assert_eq!(infer_app(&h), "unknown");
        let empty = HeaderMap::new();
        assert_eq!(infer_app(&empty), "unknown");
    }


    #[test]
    fn extract_gemini_usage_metadata() {
        // 完整形狀：candidates + thoughts 併入 out
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":1,"totalTokenCount":64,"thoughtsTokenCount":58}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 59));
        // 被截斷回包：無 candidates，用 total-prompt 回退
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":5,"totalTokenCount":18,"thoughtsTokenCount":13}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok), (5, 13));
        // 快取欄位
        let v: serde_json::Value = serde_json::from_str(
            r#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":2,"totalTokenCount":12,"cachedContentTokenCount":4}}"#,
        )
        .unwrap();
        let mut u = SseUsage::default();
        extract_usage(&v, &mut u);
        assert_eq!((u.in_tok, u.out_tok, u.cache_read), (10, 2, 4));
    }
