//! 上游目標組裝、模型來源解析、請求體轉譯與剝離。
//!
//! 從「客戶端要什麼格式」到「上游收到什麼位元組」之間的所有轉換
//! 都在這裡。`TransSpec` 只帶兩個欄位（翻譯類型、是否翻譯）是刻意的：
//! 它曾帶著 `in_fmt`/`target_fmt` 卻沒人讀，clippy 抓到後才瘦身。

use super::*;

/// 跨來源路由命中行：改走來源的連接資訊。
pub(super) struct ReroutedProvider {
    pub(super) id: i64,
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) scheme: String,
    pub(super) format: String,
}
/// 組裝上游 URL 與轉發標頭，回傳 `(url, headers)`。
///
/// - 翻譯路徑固定打 `/chat/completions`；直通保留原始路徑（`join_upstream`
///   負責去重 `/v1` 之類的重複前綴）。查詢字串會剝掉 `key=` 再附加。
/// - 標頭：先複製客戶端標頭（略過 hop-by-hop），再依渠道 `auth_scheme`
///   注入鑑權 —— `goog-key` 用 `x-goog-api-key`、`anthropic` 用 `x-api-key`
///   + 版本頭、其餘用 `Bearer`。
pub(super) fn build_upstream_target(
    parts: &axum::http::request::Parts,
    authed: &keys::AuthedKey,
    translated: bool,
    app: &str,
) -> (String, Vec<(String, String)>) {
    let mut url = if translated {
        format!(
            "{}/chat/completions",
            authed.provider_base_url.trim_end_matches('/')
        )
    } else {
        join_upstream(&authed.provider_base_url, parts.uri.path())
    };
    if let Some(q) = strip_key_param(parts.uri.query()) {
        url.push('?');
        url.push_str(&q);
    }

    let mut fwd: Vec<(String, String)> = Vec::new();
    for (k, v) in parts.headers.iter() {
        let name = k.as_str();
        if is_hop_header(name) {
            continue;
        }
        if let Ok(val) = v.to_str() {
            fwd.push((name.to_string(), val.to_string()));
        }
    }
    let pkey = authed.provider_api_key.as_str();
    match authed.provider_auth_scheme.as_str() {
        "goog-key" => {
            // Google 原生：x-goog-api-key，不用 Bearer
            if !pkey.is_empty() {
                fwd.push(("x-goog-api-key".to_string(), pkey.to_string()));
            }
        }
        "anthropic" => {
            // Anthropic 官方：x-api-key + 版本頭
            if !pkey.is_empty() {
                fwd.push(("x-api-key".to_string(), pkey.to_string()));
            }
            fwd.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        _ => {
            if !pkey.is_empty() {
                fwd.push(("authorization".to_string(), format!("Bearer {pkey}")));
            }
        }
    }

    // OpenCode Zen/Go 要求 session 標頭才能有效路由（缺失回 400 MissingSessionID）。
    // 客戶端（hermes/opencode/claude 等）沒帶時，注入穩定的合成 id：同一本地 Key
    // 固定同一 id → 上游路由與前綴快取穩定；Codex 自帶 session-id 時原樣透傳。
    if authed.provider_base_url.contains("opencode.ai") {
        const SESSION_HEADS: [&str; 6] = [
            "x-opencode-session",
            "session-id",
            "x-session-id",
            "thread-id",
            "x-client-request-id",
            "x-grok-session-id",
        ];
        let has_session = fwd
            .iter()
            .any(|(k, _)| SESSION_HEADS.iter().any(|h| k.eq_ignore_ascii_case(h)));
        if !has_session {
            let who = if app.is_empty() { "unknown" } else { app };
            let sid = format!("tg-{who}-{}", authed.id);
            fwd.push(("x-opencode-session".to_string(), sid.clone()));
            fwd.push(("session-id".to_string(), sid));
        }
    }
    // 注：X-TG-App 是網關內務頭，如需隱藏可在此剝離；當前選擇透傳以便上游觀測。
    (url, fwd)
}
/// 取渠道顯示名（僅用於錯誤訊息；查不到時給可讀的佔位字串）。
pub(super) fn provider_name(conn: &rusqlite::Connection, provider_id: i64) -> String {
    conn.query_row("SELECT name FROM providers WHERE id=?1", [provider_id], |r| {
        r.get(0)
    })
    .unwrap_or_else(|_| "（未知來源）".to_string())
}
/// 跨來源模型路由：在其他啟用來源的可用模型表中找 display/actual 命中
///（排除 Key 綁定來源，按來源優先級、id 取第一個）。
pub(super) fn resolve_model_provider(
    conn: &rusqlite::Connection,
    model: &str,
    exclude_pid: i64,
) -> Option<ReroutedProvider> {
    conn.query_row(
        "SELECT p.id, p.base_url, p.api_key, p.auth_scheme, p.api_format
         FROM provider_models m JOIN providers p ON p.id = m.provider_id
         WHERE m.enabled = 1 AND p.enabled = 1 AND p.id != ?2
           AND (lower(m.display_name) = lower(?1) OR lower(m.actual_model) = lower(?1))
         ORDER BY p.priority ASC, p.id ASC LIMIT 1",
        rusqlite::params![model, exclude_pid],
        |r| {
            Ok(ReroutedProvider {
                id: r.get(0)?,
                base_url: r.get(1)?,
                api_key: r.get(2)?,
                scheme: r.get(3)?,
                format: r.get(4)?,
            })
        },
    )
    .ok()
}
/// 翻譯決策（矩陣判定結果中，翻譯階段真正需要的部分）。
#[derive(Debug, Clone, Copy)]
pub(super) struct TransSpec {
    pub(super) kind: TransKind,
    /// 是否需要翻譯（`kind` 不是 None，且渠道協議不是原生同型）。
    pub(super) translated: bool,
}
/// 要送往上游的請求體。
pub(super) struct ForwardBody {
    pub(super) bytes: Vec<u8>,
    /// 送往上游的實際模型名（翻譯或直通改寫時）；未改寫則為空字串。
    pub(super) model: String,
}
/// `translate_forward_body` 的結果。
pub(super) enum BodyPrep {
    Ready(ForwardBody),
    /// 需要翻譯，但請求體不是合法 JSON —— 呼叫端負責留痕與回應。
    Unparsable,
}
/// 把客戶端請求體轉成要送上游的位元組。
///
/// - 翻譯路徑（Anthropic／Responses → chat）：以白名單重建 body，
///   同時把 model 由顯示名換成 actual。
/// - 直通路徑：只把 body 內既有的 `model` 字串由顯示名改寫成 actual
///   （無映射則原樣不動；Gemini 原生等無 model 的 body 完全不碰）。
/// - 最後若為 OpenAI chat 的串流請求，補 `stream_options.include_usage` ——
///   Responses／Anthropic／Gemini 加這個欄位會被上游 400，故以
///   `want_usage_opt` 區分。
pub(super) fn translate_forward_body(
    conn: &rusqlite::Connection,
    provider_id: i64,
    raw: &[u8],
    body_json: &Option<serde_json::Value>,
    model_raw: &str,
    fmt: TransSpec,
    want_usage_opt: bool,
) -> BodyPrep {
    let mut fwd_value = body_json.clone();
    let mut translated_model = String::new();
    let mut rewritten = false;

    if fmt.translated {
        let Some(v) = fwd_value.as_ref().cloned() else {
            return BodyPrep::Unparsable;
        };
        let actual = crate::models::resolve_actual(conn, provider_id, model_raw);
        fwd_value = Some(if fmt.kind == TransKind::ResponsesToChat {
            crate::translate::responses_to_openai(&v, &actual)
        } else {
            crate::translate::anthropic_to_openai(&v, &actual)
        });
        translated_model = actual;
    } else if !model_raw.is_empty() {
        // 直通改寫：僅當請求體本來就帶 model 字串才動
        if let Some(obj) = fwd_value.as_mut().and_then(|v| v.as_object_mut()) {
            if obj.get("model").and_then(|m| m.as_str()).is_some() {
                let actual = crate::models::resolve_actual(conn, provider_id, model_raw);
                if !actual.is_empty() && actual != model_raw {
                    obj.insert(
                        "model".to_string(),
                        serde_json::Value::String(actual.clone()),
                    );
                    translated_model = actual;
                    rewritten = true;
                }
            }
        }
    }

    let mut bytes = if fmt.translated || rewritten {
        serde_json::to_vec(fwd_value.as_ref().expect("forward body"))
            .unwrap_or_else(|_| raw.to_vec())
    } else {
        raw.to_vec()
    };
    // 串流強制索取用量（僅 OpenAI chat 需要；見上方說明）
    if want_usage_opt {
        if let Some(v) = fwd_value.as_ref() {
            if v.get("stream").and_then(|s| s.as_bool()).unwrap_or(false)
                && v.get("stream_options").is_none()
            {
                let mut owned = v.clone();
                if let Some(obj) = owned.as_object_mut() {
                    obj.insert(
                        "stream_options".to_string(),
                        serde_json::json!({"include_usage": true}),
                    );
                    bytes = serde_json::to_vec(&owned).unwrap_or(bytes);
                }
            }
        }
    }
    BodyPrep::Ready(ForwardBody {
        bytes,
        model: translated_model,
    })
}
/// 依序套用所有「上游已知拒收」的剝離：
/// ① 該渠道學到的拒收欄位（SQLite 持久化記憶，見 §3 B2）
/// ② OpenAI 專屬推理簽章 `encrypted_content`（§3 B1）
/// ③ 上游不支援的 tool types（Codex `custom` → DeepSeek 400）
/// ④ 直通 chat 的歷史配對修復（舊 session 孤兒 tool_calls；有效歷史不動）
///
/// 每個步驟都是「有改才換、沒改不動」，所以全部 no-op 時 body 原樣送出。
pub(super) fn strip_for_upstream(
    conn: &rusqlite::Connection,
    provider_id: i64,
    mut body: Vec<u8>,
    in_fmt: InFmt,
    translated: bool,
) -> Vec<u8> {
    for f in trace::load_stripped(conn, provider_id) {
        if let Some(nb) = strip_json_field(&body, &f) {
            body = nb;
        }
    }
    if let Some(stripped) = strip_encrypted_content(&body) {
        body = stripped;
    }
    if !translated {
        if let Some(stripped) = strip_unsupported_tools(&body) {
            body = stripped;
        }
        if let Some(fixed) = sanitize_passthrough_chat_body(&body, in_fmt, translated) {
            body = fixed;
        }
    }
    body
}
