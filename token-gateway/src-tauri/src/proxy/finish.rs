//! 回應收尾：標頭轉發、串流分支、非流式整包解析、記帳。

use super::*;
use crate::translate::SseUsage;

/// 回應收尾階段所需的請求脈絡。
///
/// 共用的 4 個欄位（`ctx`／`started`／`app`／`model_raw`）來自 [`ReqCtx`]；
/// 這裡只補收尾額外需要的欄位。
pub(super) struct FinishCtx<'a> {
    pub(super) req: ReqCtx<'a>,
    /// 送往上游的實際模型名（翻譯或直通改寫時）；未改寫則為空字串。
    pub(super) translated_model: &'a str,
    pub(super) body_json: &'a Option<serde_json::Value>,
    /// 本地 Key 的 id；直連上游模式為 None。
    pub(super) key_id: Option<i64>,
    pub(super) provider_id: i64,
    pub(super) translated: bool,
    pub(super) kind: TransKind,
}
/// 收尾：把上游回應轉回客戶端要的形狀、寫用量日誌、回傳響應。
///
/// - **串流**：依 `kind`／`translated` 選一個轉送分支，各自 spawn 一個 task
///   邊收邊轉邊送，流結束時記帳（見 `relay_sse` / `StreamLog`）。
/// - **非流式**：整包讀完再轉換形狀。只有成功狀態才轉 —— 錯誤回包必須原樣
///   透出，因為客戶端是靠狀態碼與 `error` 欄位讀訊息的。
///
/// 注意：每個 spawn 分支都必須在 `async move` **之外**先把要用的字串
/// 複製成 owned（`f.app.to_string()` 等）。直接把 `f` 搬進 task 不行
/// ——`f` 是借用，`tokio::spawn` 要求 `'static`。
pub(super) async fn finish_response(upstream: reqwest::Response, f: &FinishCtx<'_>) -> Response {
    let status = upstream.status();
    let latency_ms = f.req.started.elapsed().as_millis() as i64;
    let is_sse = upstream
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("text/event-stream");

    // 轉發上游標頭（略過 hop-by-hop 與 content-encoding —— 後者是因為我們
    // 已經要了 identity，且 body 可能被改寫過）
    let mut builder = Response::builder().status(status);
    for (k, v) in upstream.headers().iter() {
        let name = k.as_str();
        if is_hop_header(name) || name == "content-encoding" {
            continue;
        }
        builder = builder.header(name, v.clone());
    }

    let key_id = f.key_id;
    let provider_id = f.provider_id;
    let db_path = f.req.ctx.db_path.clone();
    // 記帳模型：送往上游的實際模型（翻譯或直通改寫時），否則用客戶端模型
    //（上游回顯優先，見下方 usage.model）
    let log_fallback = if !f.translated_model.is_empty() {
        f.translated_model.to_string()
    } else {
        f.req.model_raw.to_string()
    };

    // Codex 自訂（freeform）工具名：上游端已被 function 化送出
    //（見 translate::responses_to_openai），回程把呼叫還原成 custom_tool_call。
    let custom_tools: Vec<String> = if f.kind == TransKind::ResponsesToChat {
        f.body_json
            .as_ref()
            .map(crate::translate::custom_tool_names)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if is_sse {
        // 流式：邊轉邊解析，結束寫日誌（翻譯分支按 kind 轉換事件形狀）
        let (tx, rx) =
            tokio::sync::mpsc::channel::<Result<axum::body::Bytes, axum::Error>>(64);
        if f.kind == TransKind::ResponsesToChat {
            // Responses 流式：chat chunk → Responses 事件；流結束補發 completed/failed
            let appv = f.req.app.to_string();
            let echo = f.req.model_raw.to_string();
            let logm = log_fallback.clone();
            let customs = custom_tools.clone();
            tokio::spawn(async move {
                let on_line = |s: &mut ResponsesRelay, t: &str| {
                    responses_line_events(t, &mut s.tstate, &echo, &mut s.acc, &customs)
                };
                let on_finish = |s: &mut ResponsesRelay| {
                    crate::translate::responses_stream_finish(
                        &mut s.tstate,
                        &echo,
                        &s.acc,
                        &customs,
                    )
                };
                let st =
                    relay_sse(upstream, &tx, ResponsesRelay::default(), on_line, on_finish)
                        .await;
                StreamLog::new(
                    &db_path,
                    &appv,
                    provider_id,
                    key_id,
                    latency_ms,
                    status.as_u16(),
                )
                .write(&logm, &st.acc);
            });
            return sse_response(builder, rx);
        }
        if f.translated {
            let appv = f.req.app.to_string();
            let echo = f.req.model_raw.to_string();
            let logm = log_fallback.clone();
            tokio::spawn(async move {
                let on_line = |s: &mut AnthropicRelay, t: &str| {
                    let Some(d) = t.strip_prefix("data:") else {
                        return (vec![], false);
                    };
                    let d = d.trim_start();
                    if d.is_empty() || d == "[DONE]" {
                        return (vec![], false);
                    }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(d) else {
                        return (vec![], false);
                    };
                    // 上游錯誤 → Anthropic error 事件並收流
                    if let Some(err) = v.get("error") {
                        let err = if err.is_null() { v.clone() } else { err.clone() };
                        return (vec![crate::translate::anthropic_error_line(&err)], true);
                    }
                    (
                        crate::translate::openai_chunk_to_anthropic(
                            &mut s.tstate,
                            &v,
                            &echo,
                            &mut s.acc,
                        ),
                        false,
                    )
                };
                let on_finish = |_s: &mut AnthropicRelay| Vec::new();
                let st =
                    relay_sse(upstream, &tx, AnthropicRelay::default(), on_line, on_finish)
                        .await;
                StreamLog::new(
                    &db_path,
                    &appv,
                    provider_id,
                    key_id,
                    latency_ms,
                    status.as_u16(),
                )
                .write(&logm, &st.acc);
            });
            return sse_response(builder, rx);
        }
        let appv = f.req.app.to_string();
        let mr = f.req.model_raw.to_string();
        tokio::spawn(async move {
            let mut acc = SseAcc::new();
            let mut stream = upstream.bytes_stream();
            use tokio_stream::StreamExt;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        acc.feed(&chunk);
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(axum::Error::new(e))).await;
                        break;
                    }
                }
            }
            acc.finish();
            let u = acc.usage;
            let model = u.model.clone().unwrap_or(mr);
            StreamLog::new(
                &db_path,
                &appv,
                provider_id,
                key_id,
                latency_ms,
                status.as_u16(),
            )
            .write(&model, &u);
        });
        return sse_response(builder, rx);
    }

    // 非流式：整包解析後轉發（翻譯分支轉成 Anthropic message）
    let resp_bytes = match upstream.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return reject(
                f.req.ctx,
                f.req.started,
                f.req.app,
                f.req.model_raw,
                StatusCode::BAD_GATEWAY,
                format!("讀取上游響應失敗：{e}"),
            )
        }
    };
    let mut usage = SseUsage::default();
    let mut out_bytes = resp_bytes.to_vec();
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&resp_bytes) {
        extract_usage(&v, &mut usage);
        // 僅成功狀態才轉換形狀；錯誤回包原樣透出（客戶端按狀態碼讀錯誤）
        if f.translated && status.is_success() {
            if f.kind == TransKind::ResponsesToChat {
                let r = crate::translate::openai_to_responses(&v, f.req.model_raw, &custom_tools);
                out_bytes = serde_json::to_vec(&r).unwrap_or(out_bytes);
            } else {
                let a = crate::translate::openai_to_anthropic(&v, f.req.model_raw);
                out_bytes = serde_json::to_vec(&a).unwrap_or(out_bytes);
            }
        }
    }
    let model = usage.model.clone().unwrap_or(log_fallback);
    let norm = normalize_model(&model);
    // 這裡開短命連線（而非借用呼叫端的）：`&Connection` 不是 Send，
    // 會讓整個 future 變成 !Send（詳見 send_with_strip_retry 的註解）。
    if let Ok(conn) = open_conn(&db_path) {
        let ts = now_ms();
        let rates = crate::usage::resolve_rates(&conn, Some(provider_id), &norm, ts);
        let cost = crate::usage::rate_cost(
            &rates,
            usage.in_tok,
            usage.out_tok,
            usage.cache_read,
            usage.cache_write,
        );
        let _ = insert_log(
            &conn,
            ts,
            key_id,
            f.req.app,
            Some(provider_id),
            &model,
            &norm,
            &usage,
            cost,
            latency_ms,
            status.as_u16() as i64,
            false,
        );
        if let Some(kid) = key_id {
            let _ = keys::add_used(&conn, kid, usage.total());
        }
    }
    builder.body(Body::from(out_bytes)).unwrap_or_else(|_| {
        err_json(StatusCode::INTERNAL_SERVER_ERROR, "構造響應失敗".into())
    })
}
