//! M3：本地網關代理（`127.0.0.1:<port>`，純透傳）。
//!
//! - 鑑權：`Authorization: Bearer sk-local-…` → 查 `local_keys`（哈希比對、
//!   啟用/過期/配額/模型白名單/應用程式白名單）→ 按 Key 綁定的上游渠道轉發。
//! - 用量：非流式直接讀 JSON `usage`；流式邊轉邊解析 SSE（OpenAI chat /
//!   Responses / Anthropic 三種形狀），請求結束寫 `request_logs`。
//! - 不做格式轉換（M3 透傳；轉換列入 P1）。金鑰只進記憶體，不寫日誌。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use reqwest::Client;
use serde::Serialize;
use tokio::net::TcpListener;

use crate::fsutil::now_ms;
use crate::keys;
use crate::tools::APPS;
use crate::trace::{self, TraceRecord};

// ------------------------------------------------------------ 子模組地圖 ---
//
// 本檔案原本是 **4,195 行的單一檔案**（含一個 909 行的 `proxy_handler`）。
// Phase 2 拆成下面九個子模組（`proxy/` 子目錄，不需要 `mod.rs`）：
//
// | 模組 | 職責 |
// |---|---|
// | `matrix` | 格式矩陣：入站格式 × 渠道協議 → 翻譯類型（30 格全列舉） |
// | `strip` | 請求體手術：未知欄位解析、各種「上游拒收就剝掉」 |
// | `util` | 通用小工具：連線、模型名正規化、來源判定、標頭與路徑處理 |
// | `logging` | 用量解析與落庫、被拒日誌、錯誤回應樣板 |
// | `stream` | SSE 轉送骨架、事件形狀轉換、串流結束記帳 |
// | `forward` | 上游目標組裝、模型來源解析、請求體轉譯與剝離 |
// | `retry` | 送出上游與「400 剝離重試」 |
// | `finish` | 回應收尾：標頭轉發、串流分支、非流式整包解析、記帳 |
// | `pipeline` | 請求管線各階段（`prelude` → `request_meta` → `resolve_model` → `prepare_request` → `upstream_for`） |
// | `tests` | 1,907 行的 e2e／單元測試 |
//
// 本檔只留：常數、`ProxyCtx`／`RateLimiter`／`ProxyState`／`RunningProxy`、
// `proxy_handler`（編排，86 行）、`serve`。

mod strip;
mod util;
mod logging;
mod stream;
mod forward;
mod retry;
mod finish;
mod pipeline;
mod reqctx;
mod failover;
mod health;
mod queue;

// 子模組共用匯入：各子模組開頭的 `use super::*;` 會取得這裡的綁定，
// 因此某個模組要用兄弟模組的項目時，只要在這裡補一行即可。
// `unused_imports` 是**刻意允許**的 —— 本模組自己不一定用到每一項。
#[allow(unused_imports)]
use {
    finish::{finish_response, FinishCtx},
    forward::{
        build_upstream_target, provider_name, resolve_model_provider, resolve_model_providers,
        strip_for_upstream, translate_forward_body, BodyPrep, ForwardBody, ReroutedProvider,
        TransSpec,
    },
    failover::{learn_format, learned_format, plan_attempts, Attempt},
    health::order_by_health,
    logging::{err_json, extract_usage, insert_log, log_reject, reject, SseAcc},
    pipeline::{
        prepare_request, prelude, request_meta, resolve_model, upstream_for, PrepareInput, Prepared,
        Prelude, ReqMeta,
    },
    retry::{send_with_strip_retry, RetryCtx, SendOutcome, Upstream},
    reqctx::ReqCtx,
    stream::{
        relay_sse, responses_line_events, sse_response, AnthropicRelay, ResponsesRelay, StreamLog,
    },
    strip::{
        parse_body_json, parse_unknown_fields, sanitize_passthrough_chat_body,
        strip_encrypted_content, strip_json_field, strip_unsupported_tools, upstream_err_text,
    },
    util::{
        bearer, infer_app, is_hop_header, join_upstream, model_from_path, open_conn,
        should_inject_usage, strip_key_param,
    },
};

// 這四項原本是 `pub(crate)` 且住在 `proxy.rs`，外部模組以 `crate::proxy::X`
// 取用（`commands.rs` 用 check_port／LogRow／recent_logs，`history.rs` 用
// normalize_model）。拆檔後必須在這裡重新匯出，否則那些路徑會斷。
pub(crate) use logging::{recent_logs, LogRow};
pub(crate) use util::{check_port, normalize_model};
// 診斷命令要讀「來源健康狀態」（`commands/diagnostics.rs` 的 `source_health`）。
// `pub use`：型別要與命令函式同樣可達，否則 `private_interfaces` 會警告。
pub use health::{for_db as health_registry, HealthRegistry};
// 故障轉移佇列的唯讀預覽（`commands/diagnostics.rs` 的 `failover_queue`／`failover_models`）。
// 用 `pub use`（不是 pub(crate)）：命令函式是 `pub`，型別必須同樣可達，
// 否則會觸發 `private_interfaces` 警告。
pub use queue::{queue_models, queue_rows, QueueModel, QueueRow};

mod matrix;

use matrix::{resolve_trans_kind, InFmt, TargetFmt, TransKind};

// ---------------------------------------------------------------- 狀態 ---

/// 上游連線階段上限（秒）。握手卡住時快速失敗，避免佔用請求 10 分鐘。
pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 10;
/// 上游整體請求上限（秒），含串流讀取。
pub(crate) const REQUEST_TIMEOUT_SECS: u64 = 600;
/// `proxy_trace` 保留筆數上限（網關啟動時裁剪）。
pub(crate) const TRACE_KEEP: i64 = 5000;

#[derive(Clone)]
pub(crate) struct ProxyCtx {
    pub db_path: PathBuf,
    pub client: Client,
    pub rate: RateLimiter,
    /// 來源健康狀態（斷路器）：跨請求記住「哪個來源剛剛連續失敗」，
    /// 冷卻中的來源在候選佇列裡會被排到最後。見 `proxy/health.rs`。
    pub health: HealthRegistry,
}

#[derive(Clone, Default)]
pub(crate) struct RateLimiter {
    inner: Arc<Mutex<HashMap<i64, VecDeque<Instant>>>>,
}

impl RateLimiter {
    /// true = 放行並計數；qpm <= 0 不限流。
    fn check(&self, key_id: i64, qpm: i64) -> bool {
        if qpm <= 0 {
            return true;
        }
        let now = Instant::now();
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let q = map.entry(key_id).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t).as_secs() >= 60) {
            q.pop_front();
        }
        if q.len() as i64 >= qpm {
            return false;
        }
        q.push_back(now);
        true
    }
}

pub struct ProxyState {
    pub inner: Mutex<Option<RunningProxy>>,
}

impl Default for ProxyState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

pub struct RunningProxy {
    pub port: u16,
    pub started_at: i64,
    pub handle: tokio::task::JoinHandle<()>,
}

async fn proxy_handler(State(ctx): State<ProxyCtx>, req: axum::http::Request<Body>) -> Response {
    let started = Instant::now();
    let (parts, body) = req.into_parts();
    // 來源判定只做一次（原本在每個拒絕分支各算一次，共 6 次）
    let app = infer_app(&parts.headers);

    // ---- 鑑權 → 限流 → 讀請求體（見 prelude）
    let Prelude {
        mut authed,
        bytes,
        conn,
    } = match prelude(&ctx, &started, &app, &parts.headers, body).await {
        Ok(p) => p,
        Err(resp) => return *resp,
    };

    // ---- 請求元資料（模型名／路徑／內容類型；見 request_meta）
    let ReqMeta {
        body_json,
        path_hint,
        content_type,
        model_raw,
    } = request_meta(&parts, &bytes);
    // 共用脈絡：後續每個階段（解析來源／準備／重試／收尾）都要這 4 個值，
    // 集中在 `ReqCtx` 建一次，各階段自己的 ctx 再嵌入它。
    let rc = ReqCtx::new(&ctx, &started, &app, &model_raw);
    // ---- 模型白名單 + 來源解析（見 resolve_model）
    if let Err(resp) = resolve_model(&ctx, &started, &app, &conn, &model_raw, &mut authed) {
        return *resp;
    }

    // ---- 候選（來源, 協議）佇列 ----
    // 協議可能**逐模型**不同（opencode-go 實測：grok-4.7 只在 /responses、mimo-v2.6
    // 只在 /chat/completions），所以同一來源也要換協議重試；其他登記了同一個模型
    // 的來源一併排進佇列，當前來源連不上或 5xx 時可以換手。見 `failover.rs`。
    let in_fmt = InFmt::from_path(&path_hint);
    let others = resolve_model_providers(&conn, &model_raw, authed.provider_id);
    let attempts = plan_attempts(
        authed.provider_id,
        &authed.provider_api_format,
        in_fmt,
        learned_format(
            ctx.db_path.as_path(),
            authed.provider_id,
            &model_raw,
            &authed.provider_api_format,
        )
        .as_deref(),
        &others,
    );
    // 斷路器：連續失敗的來源排到最後（一個都不丟，見 `health.rs`）
    let attempts = order_by_health(attempts, &ctx.health);
    let total = attempts.len();
    let mut last_reject: Option<Box<Response>> = None;
    let mut success: Option<(reqwest::Response, Prepared, bool)> = None;
    for (i, at) in attempts.iter().enumerate() {
        if at.provider_id != authed.provider_id {
            match others.iter().find(|p| p.id == at.provider_id) {
                Some(p) => {
                    authed.provider_id = p.id;
                    authed.provider_base_url = p.base_url.clone();
                    authed.provider_api_key = p.api_key.clone();
                    authed.provider_auth_scheme = p.scheme.clone();
                    authed.provider_api_format = p.format.clone();
                }
                None => continue,
            }
        }
        // ---- 格式矩陣 + 請求體轉譯與剝離（見 prepare_request）
        let prep = match prepare_request(PrepareInput {
            req: rc,
            conn: &conn,
            provider_id: at.provider_id,
            api_format: &at.api_format,
            content_type: &content_type,
            path_hint: &path_hint,
            raw: &bytes,
            body_json: &body_json,
        }) {
            Ok(p) => p,
            // 第一個候選的失敗就是使用者的失敗（照原樣回報）；換協議／換來源後
            // 矩陣不支援（例如 Anthropic→Responses 沒有翻譯器）只是這條路走不通。
            Err(resp) => {
                if i == 0 {
                    return *resp;
                }
                continue;
            }
        };
        let translated = prep.translated;
        // ---- 組裝上游請求（翻譯時固定打 /chat/completions）
        let up = upstream_for(&ctx, &parts, &authed, translated, &app);
        let outcome = send_with_strip_retry(
            &RetryCtx {
                req: rc,
                content_type: &content_type,
                in_fmt: prep.in_fmt,
                target_fmt: prep.target_fmt,
                kind: prep.kind,
            },
            at.provider_id,
            &up,
            prep.bytes.clone(),
        )
        .await;
        match outcome {
            SendOutcome::Ok(r) if r.status().is_server_error() => {
                // 上游 5xx = 來源不健康（但不代表不能用：照舊把回應交給收尾階段）
                ctx.health
                    .record_failure(at.provider_id, &format!("上游 {}", r.status().as_u16()));
                if i + 1 < total {
                    trace::log_to(
                        &ctx.db_path,
                        &TraceRecord {
                            app: app.clone(),
                            model_raw: model_raw.clone(),
                            in_fmt: prep.in_fmt.as_str().to_string(),
                            target_fmt: prep.target_fmt.as_str().to_string(),
                            trans_kind: prep.kind.as_str().to_string(),
                            upstream_status: r.status().as_u16(),
                            latency_ms: started.elapsed().as_millis() as i64,
                            note: format!(
                                "上游 {}：換下一個候選（來源 {}／協議 {}）重試",
                                r.status().as_u16(),
                                at.provider_id,
                                at.api_format
                            ),
                            ..Default::default()
                        }
                        .warn(),
                    );
                } else {
                    success = Some((r, prep, translated));
                    break;
                }
            }
            SendOutcome::Ok(r) => {
                // 來源活著（含上游自己的 4xx）：斷路器歸零。
                ctx.health.record_success(at.provider_id);
                // 記住「這個來源的這個模型用這個協議會通」，下次第一個就試它。
                learn_format(
                    ctx.db_path.as_path(),
                    at.provider_id,
                    &model_raw,
                    &authed.provider_api_format,
                    &at.api_format,
                );
                success = Some((r, prep, translated));
                break;
            }
            SendOutcome::ProtocolMismatch => {
                // 協議不支援**不算**來源不健康（來源活著、只是這個模型不在這個端點）
                last_reject = None;
            }
            SendOutcome::Reject { resp, retryable } => {
                // 只有「連不上」算來源不健康；矩陣／剝離造成的拒絕不算。
                if resp.status() == StatusCode::BAD_GATEWAY {
                    ctx.health.record_failure(at.provider_id, "上游連接失敗");
                }
                if retryable && i + 1 < total {
                    last_reject = Some(resp);
                } else {
                    return *resp;
                }
            }
        }
    }
    let (upstream, prep, translated) = match success {
        Some(v) => v,
        None => {
            return match last_reject {
                Some(r) => *r,
                None => reject(
                    &ctx,
                    &started,
                    &app,
                    &model_raw,
                    StatusCode::BAD_REQUEST,
                    format!(
                        "模型 {model_raw} 在來源的所有協議下都不被接受（試過 {total} 個候選）：\
                         上游回報此模型不支援這些協議，請換模型或換來源"
                    ),
                ),
            };
        }
    };
    let translated_model = prep.model;
    finish_response(
        upstream,
        &FinishCtx {
            req: rc,
            translated_model: &translated_model,
            body_json: &body_json,
            key_id: if authed.direct { None } else { Some(authed.id) },
            provider_id: authed.provider_id,
            translated,
            kind: prep.kind,
        },
    )
    .await
}

pub async fn serve(db_path: PathBuf, listener: TcpListener) -> Result<(), String> {
    // 追蹤表上限：啟動時裁剪一次，避免長期運行無限成長。
    if let Ok(conn) = open_conn(&db_path) {
        if let Err(e) = trace::prune_traces(&conn, TRACE_KEEP) {
            eprintln!("gateway: 裁剪 proxy_trace 失敗: {e}");
        }
    }
    // 健康狀態以資料庫為範圍（見 health.rs）：先取再進 struct，避免借用已搬移的值。
    let health = health::for_db(&db_path);
    let ctx = ProxyCtx {
        db_path,
        client: Client::builder()
            // 連線階段上限：上游 TCP 可達但握手卡住時，不該等滿整體 timeout（§3 B9）
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            // 整體請求上限（含串流讀取）
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| e.to_string())?,
        rate: RateLimiter::default(),
        health,
    };
    let app = axum::Router::new()
        .fallback(proxy_handler)
        .with_state(ctx);
    axum::serve(listener, app).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- 單測 ---

#[cfg(test)]
mod tests;
