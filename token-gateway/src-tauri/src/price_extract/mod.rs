//! 價格識別：用已加入來源的模型抽取定價頁（文字/截圖/URL 代抓）。
//!
//! - Key 只在後端使用，絕不回傳前端；本次抽取不寫 `request_logs`。
//! - MVP 僅支援 OpenAI 兼容抽取來源（openai-chat / openai-responses / mixed）。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// 抽取結果（前端確認後才調用 pp_upsert / pricing_upsert 寫入）。
#[derive(Debug, Clone, Serialize)]
pub struct PriceExtractResult {
    pub in_pm: f64,
    pub out_pm: f64,
    pub cache_read_pm: f64,
    pub cache_create_pm: f64,
    pub currency: String,
    pub confidence: f64,
    pub evidence: String,
    /// 實際向上游請求的模型 ID。
    pub used_model: String,
    /// URL 代抓成功時的實際來源（頁面標題或 URL）。
    pub source_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmPrice {
    in_pm: f64,
    out_pm: f64,
    #[serde(default)]
    cache_read_pm: Option<f64>,
    #[serde(default)]
    cache_create_pm: Option<f64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    evidence: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"你是定價抽取器。從使用者提供的定價頁內容中提取指定模型的價格。
只輸出嚴格 JSON（不要 markdown、不要解釋），格式：
{"in_pm": 數字, "out_pm": 數字, "cache_read_pm": 數字, "cache_create_pm": 數字, "currency": "USD", "confidence": 0-1, "evidence": "原文片段<=200字"}
規則：
- 單位一律換算為 美元/百萬 tokens。若原文是 CNY，先按 1 USD=7.1 CNY 換算並在 evidence 註明原文幣種與數值。
- 找不到快取價格填 0；找不到輸入/輸出價格則 confidence <= 0.4 並在 evidence 說明缺失。
- 頁面含多個模型時只取使用者指定的目標模型（名稱需模糊匹配，忽略大小寫與日期尾綴如 -0731）。
- evidence 必須是原文原話摘錄，不得編造。"#;

/// HTML 去標籤轉純文字（去 script/style，標籤換空格，壓縮空白）。
pub(crate) fn html_to_text(html: &str) -> String {
    let mut s = html.to_string();
    for tag in ["script", "style", "noscript"] {
        loop {
            let lower = s.to_lowercase();
            let open = lower.find(&format!("<{tag}"));
            let close = lower.find(&format!("</{tag}>"));
            match (open, close) {
                (Some(a), Some(b)) if b > a => {
                    let end = b + tag.len() + 3;
                    s.replace_range(a..end.min(s.len()), " ");
                }
                _ => break,
            }
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    // 常見實體粗解碼
    collapsed
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

/// 後端代抓 URL（15s 超時，公開定價頁通常無需 Key）。
pub(crate) fn fetch_url_text(url: &str) -> Result<(String, String), String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("網址需以 http:// 或 https:// 開頭".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("TokenGateway/1.0 price-extract")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| format!("抓取頁面失敗：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("抓取頁面失敗 HTTP {}", resp.status()));
    }
    let html = resp.text().map_err(|e| format!("讀取頁面失敗：{e}"))?;
    let text = html_to_text(&html);
    if text.chars().count() < 20 {
        return Err("頁面內容過少（可能是 JS 渲染頁），請改用手貼文字/截圖".to_string());
    }
    // 截斷 30k 字，避免 prompt 過大
    let truncated: String = text.chars().take(30_000).collect();
    Ok((truncated, url.to_string()))
}

/// 從 LLM 回包中擷取第一個 {...} JSON 段。
pub(crate) fn extract_json_slice(s: &str) -> Result<&str, String> {
    let start = s.find('{').ok_or("模型回包無 JSON".to_string())?;
    let end = s.rfind('}').ok_or("模型回包無 JSON".to_string())?;
    if end <= start {
        return Err("模型回包無 JSON".to_string());
    }
    Ok(&s[start..=end])
}

fn validate_price(p: LlmPrice, used_model: &str, source_note: &str) -> Result<PriceExtractResult, String> {
    for (k, v) in [
        ("輸入", p.in_pm),
        ("輸出", p.out_pm),
        ("快取讀", p.cache_read_pm.unwrap_or(0.0)),
        ("快取建", p.cache_create_pm.unwrap_or(0.0)),
    ] {
        if !v.is_finite() || v < 0.0 || v > 1_000_000.0 {
            return Err(format!("{k}價格不合法：{v}"));
        }
    }
    let conf = p.confidence.unwrap_or(0.5).clamp(0.0, 1.0);
    let mut evidence: String = p.evidence.unwrap_or_default().chars().take(200).collect();
    if evidence.trim().is_empty() {
        evidence = "（模型未提供原文證據，請人工核對）".to_string();
    }
    Ok(PriceExtractResult {
        in_pm: p.in_pm,
        out_pm: p.out_pm,
        cache_read_pm: p.cache_read_pm.unwrap_or(0.0),
        cache_create_pm: p.cache_create_pm.unwrap_or(0.0),
        currency: p.currency.unwrap_or_else(|| "USD".to_string()),
        confidence: conf,
        evidence,
        used_model: used_model.to_string(),
        source_note: source_note.to_string(),
    })
}

/// 讀段產物（短鎖內取得，網路段只讀不寫）。
pub(crate) struct ExtractCtx {
    pub base: String,
    pub key: String,
    pub scheme: String,
    pub actual: String,
}

/// 短鎖段：校驗模型並讀來源憑證（Key 不離開後端，不做網路）。
pub(crate) fn extract_read_ctx(
    conn: &Connection,
    provider_id: i64,
    model: &str,
) -> Result<ExtractCtx, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("請先選擇抽取用模型".to_string());
    }
    let (base, key, scheme, format): (String, String, String, String) = conn
        .query_row(
            "SELECT base_url, api_key, auth_scheme, api_format FROM providers WHERE id = ?1",
            [provider_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| format!("抽取來源不存在：{provider_id}"))?;
    if !["openai-chat", "openai-responses", "mixed"].contains(&format.as_str()) {
        return Err("抽取用來源需為 OpenAI 兼容格式（openai-chat / openai-responses / mixed），請換一個來源".to_string());
    }
    if key.trim().is_empty() {
        return Err("該抽取來源尚未填寫 API Key".to_string());
    }
    let actual = crate::models::resolve_actual(conn, provider_id, model);
    let actual = if actual.trim().is_empty() {
        model.to_string()
    } else {
        actual
    };
    Ok(ExtractCtx {
        base,
        key,
        scheme,
        actual,
    })
}

/// 純網路段：代抓/貼文 + LLM 抽取（不碰 DB，可安全放在鎖外）。
pub(crate) fn extract_run(
    ctx: &ExtractCtx,
    url: Option<&str>,
    pasted_text: Option<&str>,
    image_base64: Option<&str>,
    target_hint: Option<&str>,
) -> Result<PriceExtractResult, String> {
    let ExtractCtx {
        base,
        key,
        scheme,
        actual,
    } = ctx;
    let scheme = scheme.as_str();
    let pasted = pasted_text.unwrap_or("").trim();
    let url_s = url.unwrap_or("").trim();
    let has_img = image_base64.map(|s| !s.trim().is_empty()).unwrap_or(false);
    if url_s.is_empty() && pasted.is_empty() && !has_img {
        return Err("請貼上網址、文字或截圖（三選一即可）".to_string());
    }
    if let Some(img) = image_base64 {
        if img.trim().len() > 7_000_000 {
            return Err("截圖過大（限約 5MB），請壓縮後重試".to_string());
        }
    }
    if pasted.chars().count() > 50_000 {
        return Err("貼上文字過長（限 5 萬字），請刪減後重試".to_string());
    }

    // 組來源文字
    let mut parts: Vec<String> = vec![];
    let mut source_note = String::new();
    if !url_s.is_empty() {
        match fetch_url_text(url_s) {
            Ok((t, note)) => {
                source_note = format!("代抓：{note}");
                parts.push(format!("【網址代抓內容】\n{t}"));
            }
            Err(e) => {
                // 有手貼/截圖時降級繼續，否則直接報錯
                if pasted.is_empty() && !has_img {
                    return Err(e);
                }
                parts.push(format!("【網址代抓失敗：{e}；以下為手貼內容】"));
            }
        }
    }
    if !pasted.is_empty() {
        let t: String = pasted.chars().take(50_000).collect();
        parts.push(format!("【手貼定價內容】\n{t}"));
    }
    if has_img && pasted.is_empty() && url_s.is_empty() {
        parts.push("【僅截圖，請從圖片中識別定價】".to_string());
    } else if has_img {
        parts.push("【另附截圖，請結合圖片核對】".to_string());
    }
    let target = target_hint.unwrap_or("").trim();
    let target_line = if target.is_empty() {
        "目標模型：頁面主體模型（如有多個，取第一個）。".to_string()
    } else {
        format!("目標模型：{target}（多模型頁只取此模型，忽略大小寫與日期尾綴）。")
    };
    let user_text = format!("{target_line}\n\n{}", parts.join("\n\n"));

    // 組 chat 請求（含可選圖片）
    let mut content: Vec<serde_json::Value> = vec![serde_json::json!({"type": "text", "text": user_text})];
    if let Some(img) = image_base64 {
        let img = img.trim();
        if !img.is_empty() {
            let data_url = if img.starts_with("data:") {
                img.to_string()
            } else {
                format!("data:image/png;base64,{img}")
            };
            content.push(serde_json::json!({"type": "image_url", "image_url": {"url": data_url}}));
        }
    }
    let body = serde_json::json!({
        "model": actual,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": content}
        ],
        "temperature": 0.1,
        "max_tokens": 800
    });
    let endpoint = format!("{}/chat/completions", base.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&endpoint).json(&body);
    if scheme == "goog-key" {
        req = req.header("x-goog-api-key", key.trim());
    } else if scheme == "anthropic" {
        req = req.header("x-api-key", key.trim());
    } else {
        req = req.bearer_auth(key.trim());
    }
    let resp = req.send().map_err(|e| format!("抽取請求失敗：{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let snippet: String = resp.text().unwrap_or_default().chars().take(200).collect();
        if status.as_u16() == 400 && snippet.contains("image") {
            return Err(format!("抽取失敗 HTTP {status}：該模型可能不支援看圖，請換 vision 模型或只貼文字。{snippet}"));
        }
        return Err(format!("抽取失敗 HTTP {status}：{}", snippet.trim()));
    }
    let v: serde_json::Value = resp.json().map_err(|e| format!("解析抽取回包失敗：{e}"))?;
    let content_str = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        // 字串就直接用，其餘（陣列／物件）序列化回字串
        .map(|c| c.as_str().map_or_else(|| c.to_string(), str::to_string))
        .unwrap_or_default();
    if content_str.trim().is_empty() {
        return Err("模型回包為空，請換個模型重試".to_string());
    }
    let slice = extract_json_slice(&content_str)?;
    let parsed: LlmPrice =
        serde_json::from_str(slice).map_err(|e| format!("抽取結果非合法 JSON（{e}），請重試"))?;
    validate_price(parsed, actual, &source_note)
}

/// 相容入口（測試用）：讀段 + 網路段依序執行。
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn price_extract(
    conn: &Connection,
    provider_id: i64,
    model: &str,
    url: Option<&str>,
    pasted_text: Option<&str>,
    image_base64: Option<&str>,
    target_hint: Option<&str>,
) -> Result<PriceExtractResult, String> {
    let ctx = extract_read_ctx(conn, provider_id, model)?;
    extract_run(&ctx, url, pasted_text, image_base64, target_hint)
}

#[cfg(test)]
mod tests;

