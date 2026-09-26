//! 剝離與直通修復（B1／B2 的規則）
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
    fn strip_unsupported_tools_keeps_function() {
        let body = serde_json::json!({
            "model": "deepseek-v4-flash",
            "tools": [
                {"type": "function", "function": {"name": "web_search", "parameters": {}}},
                {"type": "custom", "name": "computer_use", "parameters": {}},
                {"type": "custom", "name": "shell", "parameters": {}}
            ],
            "tool_choice": {"type": "function", "function": {"name": "web_search"}},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let tools = v["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        // tool_choice 仍存在（引用的 tool 還在）
        assert!(v.get("tool_choice").is_some());
    }


    #[test]
    fn strip_unsupported_tools_removes_all() {
        let body = serde_json::json!({
            "model": "m",
            "tools": [
                {"type": "custom", "name": "a"},
                {"type": "shell", "name": "b"}
            ],
            "tool_choice": "required",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = strip_unsupported_tools(&bytes).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("tools").is_none());
        assert!(v.get("tool_choice").is_none());
    }


    #[test]
    fn strip_unsupported_tools_no_tools() {
        let body = serde_json::json!({"model": "m", "messages": []});
        let bytes = serde_json::to_vec(&body).unwrap();
        // 沒有 tools 欄位，原樣返回
        assert!(strip_unsupported_tools(&bytes).is_none());
    }


    #[test]
    fn passthrough_chat_sanitize_repairs_and_noops() {        // 孤兒 tool_calls（舊中斷 session）→ 補佔位
        let broken = br#"{"model":"m","messages":[
            {"role":"user","content":"hi"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"f","arguments":"{}"}}]}
        ]}"#;
        let out = sanitize_passthrough_chat_body(broken, InFmt::OpenAi, false).expect("應修復");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "c1");
        // 有效歷史一字不動
        let ok_body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
        assert!(sanitize_passthrough_chat_body(ok_body, InFmt::OpenAi, false).is_none());
        // 非 chat / 已翻譯不碰
        assert!(sanitize_passthrough_chat_body(broken, InFmt::Anthropic, false).is_none());
        assert!(sanitize_passthrough_chat_body(broken, InFmt::OpenAi, true).is_none());
        assert!(sanitize_passthrough_chat_body(b"not json", InFmt::OpenAi, false).is_none());
    }


    #[test]
    fn strip_encrypted_content_removes_nested_occurrences() {
        // 第三方 responses 端點會以「encrypted_content was not issued to this
        // caller」400 拒收，故直通前必須剝離所有副本（含巢狀）。
        let body = br#"{"model":"m","input":[
            {"type":"reasoning","encrypted_content":"SECRET","summary":[]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}],
             "meta":{"encrypted_content":"ALSO_SECRET"}}
        ],"store":false}"#;
        let out = strip_encrypted_content(body).expect("應命中並剝離");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "仍有殘留: {s}");
        assert!(!s.contains("SECRET"), "簽章值未移除: {s}");
        // 其餘欄位一字不動
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["model"], "m");
        assert_eq!(v["store"], false);
        assert_eq!(v["input"].as_array().unwrap().len(), 2);
        assert_eq!(v["input"][0]["type"], "reasoning");
        assert_eq!(v["input"][1]["content"][0]["text"], "hi");
    }


    #[test]
    fn strip_encrypted_content_is_noop_when_absent() {
        // 未命中必須回 None（呼叫端據此判斷「原文一字不動」）
        assert!(strip_encrypted_content(br#"{"model":"m","input":"hi"}"#).is_none());
    }


    #[test]
    fn strip_encrypted_content_handles_arrays_and_deep_nesting() {
        let body =
            br#"{"a":[[{"encrypted_content":"x"}]],"b":{"c":{"d":{"encrypted_content":"y"}}}}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("encrypted_content"), "深層殘留: {s}");
    }


    #[test]
    fn strip_encrypted_content_leaves_similar_keys_alone() {
        // 只剝離精確鍵名，不得誤傷其他含相似字串的鍵
        let body = br#"{"my_encrypted_content_x":1,"encrypted_content":"z"}"#;
        let out = strip_encrypted_content(body).expect("應命中");
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("encrypted_content").is_none());
        assert_eq!(v["my_encrypted_content_x"], 1, "相似鍵名被誤刪");
    }


    #[test]
    fn strip_encrypted_content_ignores_non_json() {
        assert!(strip_encrypted_content(b"not json").is_none());
        assert!(strip_encrypted_content(b"").is_none());
    }
