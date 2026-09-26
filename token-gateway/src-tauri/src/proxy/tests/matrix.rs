//! 格式矩陣與錯誤字串（30 臂的固定樁）
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
    fn strip_key_and_model_from_path() {
        assert_eq!(
            strip_key_param(Some("alt=sse&key=SECRET&x=1")).as_deref(),
            Some("alt=sse&x=1")
        );
        assert_eq!(strip_key_param(Some("key=SECRET")), None);
        assert_eq!(strip_key_param(None), None);
        assert_eq!(
            model_from_path("/v1beta/models/gemini-3.6-flash:generateContent").as_deref(),
            Some("gemini-3.6-flash")
        );
        assert_eq!(model_from_path("/v1/chat/completions"), None);
    }


    #[test]
    fn trans_kind_matrix_is_fully_pinned() {
        use InFmt::*;
        use TargetFmt as T;
        const NONE: Option<TransKind> = Some(TransKind::None);
        const A2C: Option<TransKind> = Some(TransKind::AnthropicToChat);
        const R2C: Option<TransKind> = Some(TransKind::ResponsesToChat);
        // 5 種入站 × 6 種渠道協議 = 30 格，全部明列。
        // 這份表就是行為契約：任何分支被重排或漏改，這裡立刻失敗。
        let table: &[(InFmt, T, Option<TransKind>)] = &[
            // Anthropic 入站
            (Anthropic, T::OpenAiChat, A2C),
            (Anthropic, T::OpenAiResponses, None),
            (Anthropic, T::Mixed, A2C),
            (Anthropic, T::Anthropic, NONE),
            (Anthropic, T::Gemini, None),
            (Anthropic, T::Unknown, None),
            // Responses 入站（Codex）
            (Responses, T::OpenAiChat, R2C),
            (Responses, T::OpenAiResponses, NONE),
            (Responses, T::Mixed, R2C),
            (Responses, T::Anthropic, None),
            (Responses, T::Gemini, None),
            (Responses, T::Unknown, NONE),
            // OpenAI Chat 入站
            (OpenAi, T::OpenAiChat, NONE),
            (OpenAi, T::OpenAiResponses, None), // B6：原為靜默直通
            (OpenAi, T::Mixed, NONE),
            (OpenAi, T::Anthropic, None),
            (OpenAi, T::Gemini, None),
            (OpenAi, T::Unknown, NONE),
            // Gemini 原生入站
            (Gemini, T::OpenAiChat, None),
            (Gemini, T::OpenAiResponses, None),
            (Gemini, T::Mixed, None),
            (Gemini, T::Anthropic, None),
            (Gemini, T::Gemini, NONE),
            (Gemini, T::Unknown, None),
            // 未知入站（中性路徑，如 /v1/models）
            (Unknown, T::OpenAiChat, NONE),
            (Unknown, T::OpenAiResponses, NONE),
            (Unknown, T::Mixed, NONE),
            (Unknown, T::Anthropic, NONE),
            (Unknown, T::Gemini, None),
            (Unknown, T::Unknown, NONE),
        ];
        assert_eq!(table.len(), 30, "矩陣必須窮舉 5×6 共 30 格");
        for (i, t, want) in table {
            let got = resolve_trans_kind(*i, *t);
            match want {
                Some(k) => assert_eq!(got.ok(), Some(*k), "({i:?}, {t:?}) 翻譯類型不符"),
                None => assert!(got.is_err(), "({i:?}, {t:?}) 應被明確拒絕"),
            }
        }
    }


    #[test]
    fn b6_chat_to_responses_endpoint_errors_loudly() {
        // 原實作落入 `_ => TransKind::None`，把 chat 請求原樣打到 responses 端點，
        // 必然失敗且訊息來自上游、難以理解。改為網關自己明確報錯。
        let e = resolve_trans_kind(InFmt::OpenAi, TargetFmt::OpenAiResponses)
            .expect_err("應明確拒絕而非靜默直通");
        assert_eq!(e, E_CHAT_TO_RESPONSES);
        assert!(e.contains("Responses"), "訊息應指出協議不符: {e}");
    }


    #[test]
    fn matrix_preserves_original_error_messages() {
        // 逐格核對既有 400 訊息語義不變（含原實作的分支優先序）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::OpenAiResponses).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        // anthropic 入站的訊息優先於「gemini 渠道」訊息（原實作順序語義）
        assert_eq!(
            resolve_trans_kind(InFmt::Anthropic, TargetFmt::Gemini).unwrap_err(),
            E_ANTHROPIC_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Responses, TargetFmt::Anthropic).unwrap_err(),
            E_REVERSE_UNSUPPORTED
        );
        assert_eq!(
            resolve_trans_kind(InFmt::Gemini, TargetFmt::OpenAiChat).unwrap_err(),
            E_GEMINI_IN_ONLY
        );
        assert_eq!(
            resolve_trans_kind(InFmt::OpenAi, TargetFmt::Gemini).unwrap_err(),
            E_GEMINI_OUT_ONLY
        );
    }


    #[test]
    fn in_fmt_from_path_matches_legacy_detection() {
        // 與原 detect_in_format 的判定完全一致
        assert_eq!(InFmt::from_path("/v1/messages"), InFmt::Anthropic);
        assert_eq!(InFmt::from_path("/v1/responses"), InFmt::Responses);
        assert_eq!(InFmt::from_path("/v1/chat/completions"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/models"), InFmt::OpenAi);
        assert_eq!(InFmt::from_path("/v1/embeddings"), InFmt::OpenAi);
        assert_eq!(
            InFmt::from_path("/v1beta/models/gemini-2.5-pro:generateContent"),
            InFmt::Gemini
        );
        assert_eq!(
            InFmt::from_path("/v1beta/models/x:streamGenerateContent"),
            InFmt::Gemini
        );
        // 中性路徑 → Unknown（透傳，不誤傷）
        assert_eq!(InFmt::from_path("/v1/health"), InFmt::Unknown);
    }


    #[test]
    fn target_fmt_from_db_covers_every_ui_value() {
        // 前端 Providers.tsx 的 API_FORMATS 必須全部被辨識，
        // 否則會落入 Unknown 而被拒服務。
        for f in [
            "openai-chat",
            "openai-responses",
            "mixed",
            "anthropic",
            "gemini",
        ] {
            assert_ne!(TargetFmt::from_db(f), TargetFmt::Unknown, "{f} 未被辨識");
            // as_str 應可往返
            assert_eq!(TargetFmt::from_db(f).as_str(), f);
        }
        assert_eq!(TargetFmt::from_db("typo"), TargetFmt::Unknown);
        assert_eq!(TargetFmt::from_db(""), TargetFmt::Unknown);
    }
