//! 請求管線的各個階段（`proxy_handler` 只負責把它們串起來）。
//!
//! 順序：`prelude`（鑑權／限流／讀體）→ `request_meta`（模型名等）
//! → `resolve_model`（白名單與跨來源路由）→ `prepare_request`
//! （矩陣／轉譯／剝離）→ `upstream_for`。

use super::*;

/// 前置階段的產物。
pub(super) struct Prelude {
    pub(super) authed: keys::AuthedKey,
    pub(super) bytes: axum::body::Bytes,
    /// 已開好的連線。**擁有**它（而非借用）是刻意的：`Connection` 是 `Send`，
    /// 所以可以安全地跨 `await` 持有；`&Connection` 不是（見 `RetryCtx`）。
    pub(super) conn: rusqlite::Connection,
}
/// 前置階段：解析 Bearer → 本地 Key（401 則回退直連上游）→ 限流 → 讀請求體。
///
/// 抽出來的理由是這一段有 **5 個拒絕出口**，每個都要「記被拒日誌 + 回錯誤
/// JSON」；混在主管線裡會讓真正的請求處理流程難以看清。
///
/// `Err` 用 `Box<Response>`（`Response` 有 128 bytes，直接當 Err 會觸發
/// clippy::result_large_err，與 `send_with_strip_retry` 一致）。
pub(super) async fn prelude(
    ctx: &ProxyCtx,
    started: &Instant,
    app: &str,
    headers: &HeaderMap,
    body: Body,
) -> Result<Prelude, Box<Response>> {
    // ---- 鑑權
    let Some(secret) = bearer(headers) else {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            "",
            StatusCode::UNAUTHORIZED,
            "缺少 Authorization：請填本地 sk-local-… 或該渠道的上游 Key",
        )));
    };
    let conn = match open_conn(&ctx.db_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(reject(
                ctx,
                started,
                app,
                "",
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("資料庫不可用：{e}"),
            )))
        }
    };
    // 本地 Key 優先；401 再回退直連上游（Bearer 即渠道自身 api_key，不經本地 Key 轉發）
    // 錯誤已改為具型別的 `CmdError`（Phase 5）：`Display` 逐字輸出原訊息，
    // 因此 HTTP 回應內容不變，但這裡多了 `kind()` 可供後續分支使用。
    let key_err = |(code, err): (u16, crate::error::CmdError)| {
        reject(
            ctx,
            started,
            app,
            "",
            StatusCode::from_u16(code).unwrap_or(StatusCode::UNAUTHORIZED),
            err.to_string(),
        )
    };
    let authed = match keys::auth_key(&conn, &secret) {
        Ok(k) => k,
        Err((401, _)) => match keys::auth_direct(&conn, &secret) {
            Ok(k) => k,
            Err(e) => return Err(Box::new(key_err(e))),
        },
        Err(e) => return Err(Box::new(key_err(e))),
    };
    if !ctx.rate.check(authed.id, authed.rate_limit_qpm) {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            "",
            StatusCode::TOO_MANY_REQUESTS,
            format!("該 Key 限流中（{}次/分鐘）", authed.rate_limit_qpm),
        )));
    }

    // 讀請求體（32MB 上限）
    const LIMIT: usize = 32 * 1024 * 1024;
    let bytes = match axum::body::to_bytes(body, LIMIT).await {
        Ok(b) => b,
        Err(_) => {
            return Err(Box::new(reject(
                ctx,
                started,
                app,
                "",
                StatusCode::PAYLOAD_TOO_LARGE,
                "請求體超過 32MB",
            )))
        }
    };
    Ok(Prelude {
        authed,
        bytes,
        conn,
    })
}
/// 模型白名單檢查 + 來源解析。
///
/// 兩件事放在一起，是因為它們共用同一組拒絕出口（模型名／來源名），
/// 且都可能在「模型不在清單內」時報錯。
///
/// - 白名單：Key 可限制可調用的模型與來源（空集合＝不限制）。
/// - 來源解析：先比對 Key 綁定來源的模型表；未命中再跨來源按模型路由
///   （其他啟用來源登記的同名模型同樣可走，Key 的額度／限流照常記在 Key 上）；
///   兩者都沒有才 400。
/// - **直連上游模式不做跨來源路由**：僅允許該渠道登記的模型。
///
/// 成功且跨來源路由命中時，會就地改寫 `authed` 的 `provider_*` 欄位。
///
/// 錯誤訊息刻意寫得囉唆：上游對「模型不存在」只回 cryptic 的 404/400，
/// 這裡直接指明「綁定來源是誰、該往哪個方向換 Key」，讓使用者能自己修。
///
/// 這裡可以安全地借用 `&Connection`：本函式是**同步**的，沒有 await 點，
/// 因此不受「`&Connection` 不是 `Send`」的限制（見 `RetryCtx`）。
pub(super) fn resolve_model(
    ctx: &ProxyCtx,
    started: &Instant,
    app: &str,
    conn: &rusqlite::Connection,
    model_raw: &str,
    authed: &mut keys::AuthedKey,
) -> Result<(), Box<Response>> {
    if !authed.allowed_models.is_empty()
        && !model_raw.is_empty()
        && !authed.allowed_models.iter().any(|m| m == model_raw)
    {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許調用模型 {model_raw}"),
        )));
    }
    if !authed.allowed_apps.is_empty()
        && app != "unknown"
        && !authed.allowed_apps.iter().any(|a| a == app)
    {
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::FORBIDDEN,
            format!("該 Key 不允許來源 {app}"),
        )));
    }

    if model_raw.is_empty() {
        return Ok(());
    }
    let list = crate::models::models_list(conn, authed.provider_id).unwrap_or_default();
    let usable: Vec<_> = list.into_iter().filter(|m| m.enabled).collect();
    let want = model_raw.to_lowercase();
    let bound_hit = usable.iter().any(|m| {
        m.display_name.to_lowercase() == want || m.actual_model.to_lowercase() == want
    });
    if usable.is_empty() || bound_hit {
        return Ok(());
    }
    if authed.direct {
        let pname = provider_name(conn, authed.provider_id);
        return Err(Box::new(reject(
            ctx,
            started,
            app,
            model_raw,
            StatusCode::BAD_REQUEST,
            format!(
                "直連模式僅允許來源「{pname}」登記的模型，{model_raw} 不在其清單內，請求不會轉發"
            ),
        )));
    }
    match resolve_model_provider(conn, model_raw, authed.provider_id) {
        Some(p) => {
            // 跨來源路由命中：改走登記該模型的來源
            authed.provider_id = p.id;
            authed.provider_base_url = p.base_url;
            authed.provider_api_key = p.api_key;
            authed.provider_auth_scheme = p.scheme;
            authed.provider_api_format = p.format;
            Ok(())
        }
        None => {
            let pname = provider_name(conn, authed.provider_id);
            Err(Box::new(reject(
                ctx,
                started,
                app,
                model_raw,
                StatusCode::BAD_REQUEST,
                format!(
                    "模型 {model_raw} 不在該 Key 綁定的來源「{pname}」模型清單內，請求不會轉發；請換用綁定正確來源的 Key；若該來源實際支援此模型，請先在來源的使用模型中新增對應映射"
                ),
            )))
        }
    }
}
/// `prepare_request` 的輸入（欄位較多，故用參數物件而非長參數列）。
pub(super) struct PrepareInput<'a> {
    pub(super) ctx: &'a ProxyCtx,
    pub(super) started: &'a Instant,
    pub(super) app: &'a str,
    pub(super) conn: &'a rusqlite::Connection,
    pub(super) provider_id: i64,
    /// 渠道協議（`providers.api_format`），用於矩陣判定。
    pub(super) api_format: &'a str,
    pub(super) model_raw: &'a str,
    pub(super) content_type: &'a str,
    pub(super) path_hint: &'a str,
    pub(super) raw: &'a [u8],
    pub(super) body_json: &'a Option<serde_json::Value>,
}
/// 請求準備的產物。
pub(super) struct Prepared {
    pub(super) in_fmt: InFmt,
    pub(super) target_fmt: TargetFmt,
    pub(super) kind: TransKind,
    pub(super) translated: bool,
    /// 已轉譯、已剝離，可以直接送上游的位元組。
    pub(super) bytes: Vec<u8>,
    /// 送往上游的實際模型名（未改寫則空字串）。
    pub(super) model: String,
}
/// 格式矩陣判定 → 請求體轉譯 → 剝離上游拒收結構。
///
/// - 矩陣由 `resolve_trans_kind` 全表窮舉（編譯器保證，見 §3 B5/B6）；
///   不支援的組合先留一筆 warn 追蹤（`trans_kind: "unsupported"`）再回 400。
/// - 翻譯路上請求體不是 JSON 時，留下**原始位元組的 hex** 再回 400 ——
///   這是 §5.2 那個懸而未決的「body 解析失敗 400」的定案依據。
/// - 最後套用 `strip_for_upstream`（渠道記憶欄位、encrypted_content、
///   不支援的 tool types、直通歷史配對修復）。
///
/// 本函式是同步的，所以可以安全借用 `&Connection`。
pub(super) fn prepare_request(input: PrepareInput<'_>) -> Result<Prepared, Box<Response>> {
    let in_fmt = InFmt::from_path(input.path_hint);
    let target_fmt = TargetFmt::from_db(input.api_format);
    let kind = match resolve_trans_kind(in_fmt, target_fmt) {
        Ok(k) => k,
        Err(msg) => {
            trace::log_to(
                &input.ctx.db_path,
                &TraceRecord {
                    app: input.app.to_string(),
                    model_raw: input.model_raw.to_string(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: "unsupported".to_string(),
                    upstream_status: 400,
                    latency_ms: input.started.elapsed().as_millis() as i64,
                    note: msg.to_string(),
                    ..Default::default()
                }
                .warn(),
            );
            return Err(Box::new(reject(
                input.ctx,
                input.started,
                input.app,
                input.model_raw,
                StatusCode::BAD_REQUEST,
                msg,
            )));
        }
    };
    let translated = kind != TransKind::None;
    // 流式請求強制索取用量：僅 OpenAI chat（含翻譯後）需要顯式 stream_options；
    // Responses / Anthropic / Gemini 加此欄位會被上游 400。
    let want_usage_opt = should_inject_usage(translated, input.path_hint);
    let (bytes, model) = match translate_forward_body(
        input.conn,
        input.provider_id,
        input.raw,
        input.body_json,
        input.model_raw,
        TransSpec { kind, translated },
        want_usage_opt,
    ) {
        BodyPrep::Ready(fb) => (fb.bytes, fb.model),
        BodyPrep::Unparsable => {
            // ── §5.2 的關鍵修復 ──
            // 本專案的「body 解析失敗 400」一直無法判定根因：docs/evidence/
            // 的三份樣本裡，成功案例的 bytes_len 是原始長度，兩個失敗案例卻是
            // 「去引號後」的長度，無法區分「真解析失敗」與「debug 儀器弄壞 body」。
            // 這裡把**原始位元組前綴的 hex** 落庫，下次失敗即可直接定案。
            trace::log_to(
                &input.ctx.db_path,
                &TraceRecord {
                    app: input.app.to_string(),
                    model_raw: input.model_raw.to_string(),
                    in_fmt: in_fmt.as_str().to_string(),
                    target_fmt: target_fmt.as_str().to_string(),
                    trans_kind: kind.as_str().to_string(),
                    upstream_status: 400,
                    latency_ms: input.started.elapsed().as_millis() as i64,
                    content_type: input.content_type.to_string(),
                    note: format!(
                        "請求體不是合法 JSON，無法翻譯（原始 {} bytes，已記錄 hex）",
                        input.raw.len()
                    ),
                    ..Default::default()
                }
                .with_body_hex(input.raw)
                .warn(),
            );
            return Err(Box::new(reject(
                input.ctx,
                input.started,
                input.app,
                input.model_raw,
                StatusCode::BAD_REQUEST,
                format!("{} 請求體不是 JSON，無法轉換為上游格式", in_fmt.label()),
            )));
        }
    };
    // 預先剝離該渠道已知拒收欄位與已知不支援結構（詳見 strip_for_upstream）。
    let bytes = strip_for_upstream(input.conn, input.provider_id, bytes, in_fmt, translated);
    Ok(Prepared {
        in_fmt,
        target_fmt,
        kind,
        translated,
        bytes,
        model,
    })
}
/// 一次請求的四個元資料，後續每個階段都要用。
pub(super) struct ReqMeta {
    /// 解析後的請求體（不是合法 JSON 時為 `None`；錯誤留給 prepare_request 回報）
    pub(super) body_json: Option<serde_json::Value>,
    /// 請求路徑，用於判定入站格式（`InFmt::from_path`）
    pub(super) path_hint: String,
    pub(super) content_type: String,
    /// 模型名。Gemini 原生請求體沒有 `model` 欄位，從 URL 路徑回填。
    pub(super) model_raw: String,
}
/// 從請求標頭與本體取出後續階段要用的四個值。
///
/// 模型名的來源順序是刻意的：**請求體優先，其次 URL 路徑**。Gemini 原生
/// 端點把模型放在路徑（`/v1beta/models/gemini-2.5-pro:generateContent`），
/// 只有路徑回填才能讓白名單與記帳拿到正確的模型名。
pub(super) fn request_meta(parts: &axum::http::request::Parts, bytes: &[u8]) -> ReqMeta {
    let body_json = parse_body_json(bytes);
    let path_hint = parts.uri.path().to_string();
    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let model_raw = body_json
        .as_ref()
        .and_then(|v| v.get("model"))
        .and_then(|m| m.as_str())
        .map(|s| s.to_string())
        .or_else(|| model_from_path(&path_hint))
        .unwrap_or_default();
    ReqMeta {
        body_json,
        path_hint,
        content_type,
        model_raw,
    }
}
/// 組裝上游連線物件（URL + 轉發標頭 + 方法 + client）。
///
/// 只是把 `build_upstream_target` 的結果（URL 與標頭）與 `client`／`method`
/// 綁成 `Upstream`，讓呼叫端少一段純粹的欄位搬運。
pub(super) fn upstream_for<'a>(
    ctx: &'a ProxyCtx,
    parts: &axum::http::request::Parts,
    authed: &keys::AuthedKey,
    translated: bool,
    app: &str,
) -> Upstream<'a> {
    let (url, headers) = build_upstream_target(parts, authed, translated, app);
    Upstream {
        client: &ctx.client,
        method: parts.method.clone(),
        url,
        headers,
    }
}
