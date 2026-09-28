//! 送出上游與「400 剝離重試」。
//!
//! ⚠️ 本模組的函式**不能**接受 `&rusqlite::Connection`：`Connection`
//! 是 `Send` 但**不是** `Sync`，`&Connection` 因此不是 `Send`，會讓
//! axum 的 `Handler` 靜默不成立（編譯器不會提 Send）。這裡一律開
//! 短命連線或只收 `db_path`。

use super::*;

/// 上游連線資訊，並負責實際發送。
///（把 client／method／url／headers 綁成一個物件，免得 send 變成一長串參數。）
pub(super) struct Upstream<'a> {
    pub(super) client: &'a reqwest::Client,
    pub(super) method: reqwest::Method,
    pub(super) url: String,
    pub(super) headers: Vec<(String, String)>,
}
impl Upstream<'_> {
    async fn send(&self, body: Vec<u8>) -> Result<reqwest::Response, reqwest::Error> {
        let mut b = self.client.request(self.method.clone(), &self.url);
        for (k, v) in self.headers.iter() {
            b = b.header(k, v);
        }
        // 強制 identity：上游若壓縮，我們得先解壓才能解析 SSE。
        b = b.header("accept-encoding", "identity");
        b.body(body).send().await
    }
}
/// 400 重試階段所需的請求脈絡（留痕與拒絕都要用）。
///
/// 共用的 4 個欄位（`ctx`／`started`／`app`／`model_raw`）來自 [`ReqCtx`]；
/// 這裡只補本階段額外需要的轉譯脈絡。
pub(super) struct RetryCtx<'a> {
    pub(super) req: ReqCtx<'a>,
    pub(super) content_type: &'a str,
    pub(super) in_fmt: InFmt,
    pub(super) target_fmt: TargetFmt,
    pub(super) kind: TransKind,
}
impl RetryCtx<'_> {
    /// 建立一筆本請求的追蹤記錄（尚未寫入）。
    /// `retry_count` 由「有沒有真的剝掉東西」推導 —— 有剝才叫重試。
    pub(super) fn trace(&self, status: u16, stripped: Vec<String>, note: &str) -> TraceRecord {
        TraceRecord {
            app: self.req.app.to_string(),
            model_raw: self.req.model_raw.to_string(),
            in_fmt: self.in_fmt.as_str().to_string(),
            target_fmt: self.target_fmt.as_str().to_string(),
            trans_kind: self.kind.as_str().to_string(),
            upstream_status: status,
            latency_ms: self.req.started.elapsed().as_millis() as i64,
            retry_count: if stripped.is_empty() { 0 } else { 1 },
            stripped_fields: stripped,
            content_type: self.content_type.to_string(),
            note: note.to_string(),
            ..Default::default()
        }
    }

    pub(super) fn connect_failed(&self, e: &reqwest::Error) -> Response {
        reject(
            self.req.ctx,
            self.req.started,
            self.req.app,
            self.req.model_raw,
            StatusCode::BAD_GATEWAY,
            format!("上游連接失敗：{e}"),
        )
    }
}
/// 送出請求；若上游回 400 且能從錯誤訊息解析出拒收欄位名，記住該欄位、
/// 剝離後重試一次。回傳最終要用的上游響應，或已備好的拒絕回應
/// （`Box<Response>`：`Response` 很大，直接當 Err 會讓 Result 過胖，
/// 見 clippy::result_large_err）。
///
/// 學到的欄位持久化於 SQLite 的 `provider_stripped_fields`：原實作只存在
/// 進程記憶體，網關每次停止／啟動即歸零，導致每個渠道的第一個請求都要
/// 重踩一次 400 再重試（見 §3 B2）。正常情況下 `strip_for_upstream` 已先
/// 套用這份記憶，所以不會走到這裡。
pub(super) async fn send_with_strip_retry(
    rc: &RetryCtx<'_>,
    provider_id: i64,
    up: &Upstream<'_>,
    body: Vec<u8>,
) -> Result<reqwest::Response, Box<Response>> {
    let upstream = match up.send(body.clone()).await {
        Ok(r) => r,
        Err(e) => return Err(Box::new(rc.connect_failed(&e))),
    };
    if upstream.status() != StatusCode::BAD_REQUEST {
        return Ok(upstream);
    }
    // 400 且報拒收欄位 → 記住並剝離重發一次（New-API 系 unknown field；
    // OpenRouter 系 Unsupported parameter(s)；多個一次全剝離）；
    // 仍失敗或無法解析則透出上游原文，不再吞錯。
    let eb: Vec<u8> = upstream.bytes().await.unwrap_or_default().to_vec();
    // 客戶端訊息用截斷版；追蹤表存完整原文（§5.3 第 0 層）
    let upstream_text = upstream_err_text(&eb);
    let upstream_full = String::from_utf8_lossy(&eb).to_string();
    let mut retried: Option<reqwest::Response> = None;
    let mut applied: Vec<String> = vec![];
    let fields = parse_unknown_fields(&String::from_utf8_lossy(&eb));
    if !fields.is_empty() {
        let mut nb = body.clone();
        // 用短命連線寫記憶，而不是借用呼叫端的連線：
        // `rusqlite::Connection` 是 Send 但**不是 Sync**，所以 `&Connection`
        // 跨 await 會讓整個 future 變成 !Send，axum 的 Handler 就不成立。
        // 開新連線也與 `trace::log_to` 的既有做法一致。
        let mem = open_conn(&rc.req.ctx.db_path).ok();
        for field in &fields {
            if let Some(n) = strip_json_field(&nb, field) {
                match mem.as_ref().map(|c| trace::remember_stripped(c, provider_id, field)) {
                    Some(Err(e)) => eprintln!("gateway: 記錄拒收欄位失敗: {e}"),
                    None => eprintln!("gateway: 記錄拒收欄位失敗: 資料庫不可用"),
                    Some(Ok(())) => {}
                }
                nb = n;
                applied.push(field.clone());
            }
        }
        drop(mem);
        if !applied.is_empty() {
            // 剝離事件留痕：這是回答「這個 400 到底剝了什麼」的唯一來源。
            // 記的是**原始** body，不是剝完的 nb（否則看不出剝了什麼）。
            let rec = rc
                .trace(400, applied.clone(), "上游 400 拒收欄位，已剝離並重試")
                .with_body(&body)
                .with_upstream_error(&upstream_full);
            trace::log_to(&rc.req.ctx.db_path, &rec);
            match up.send(nb).await {
                Ok(r) => retried = Some(r),
                Err(e) => return Err(Box::new(rc.connect_failed(&e))),
            }
        }
    }
    match retried {
        Some(r) if !r.status().is_client_error() => Ok(r),
        Some(r) => {
            let st = r.status();
            let eb2 = r.bytes().await.unwrap_or_default().to_vec();
            let eb2_full = String::from_utf8_lossy(&eb2).to_string();
            // 剝離後仍失敗 → 這是真正未解決的 400，完整留痕
            let rec = rc
                .trace(st.as_u16(), applied.clone(), "剝離後重試仍失敗（未解決）")
                .with_body(&body)
                .with_upstream_error(&eb2_full);
            trace::log_to(&rc.req.ctx.db_path, &rec);
            Err(Box::new(reject(
                rc.req.ctx,
                rc.req.started,
                rc.req.app,
                rc.req.model_raw,
                st,
                upstream_err_text(&eb2),
            )))
        }
        None => {
            // 無法從錯誤訊息解析出欄位名 → 相容策略失效，必須留痕才能改進
            let note = if applied.is_empty() {
                "上游 400 且無法解析出拒收欄位名（相容策略失效）"
            } else {
                "上游 400（剝離未命中任何欄位）"
            };
            let rec = rc
                .trace(400, applied.clone(), note)
                .with_body(&body)
                .with_upstream_error(&upstream_full);
            trace::log_to(&rc.req.ctx.db_path, &rec);
            Err(Box::new(reject(
                rc.req.ctx,
                rc.req.started,
                rc.req.app,
                rc.req.model_raw,
                StatusCode::BAD_REQUEST,
                upstream_text,
            )))
        }
    }
}
