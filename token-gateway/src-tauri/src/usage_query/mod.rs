//! 每來源的用量查詢（P2.1，對標 cc-switch 的 Usage Query）。
//!
//! ## cc-switch 的模型（照它文件抄）
//!
//! - 兩類：**自動顯示**（OAuth 帳號型）與**手動開啟**（內建樣板 ＋ 自訂腳本）。
//! - 每張來源卡有一個「用量查詢」面板：開關、樣板、API Key／Base URL／
//!   Access Token／User ID、逾時（預設 10 秒）、自動查詢間隔（0–1440 分，0＝關）。
//! - 面板裡有「測試腳本」：送一次請求、跑 extractor、把結果或錯誤顯示出來。
//! - 卡片底部顯示：用量百分比 ＋ 重置倒數，顏色 <70% 綠 / 70–89% 橘 / ≥90% 紅；
//!   多方案顯示方案數、可展開。
//!
//! ## 我們的版本差在哪（刻意，且寫在對齊矩陣）
//!
//! 自訂的那一格不是 JS 而是**宣告式抽取規格**（見 `extract.rs` 的說明）。
//! 其餘（開關／樣板／欄位／逾時／自動間隔／測試／卡片顯示／顏色）照做。
//!
//! 內建樣板只放**查得到、講得出**的東西：New API 中轉（cc-switch 文件裡的原文範例）
//! 與幾個公開的餘額端點；每個樣板的說明欄都寫明它預期什麼形狀的回應，
//! 並提醒使用者先用「測試」確認 —— 樣板猜錯比沒有樣板更糟（§0.9.20 的教訓）。

mod extract;
mod run;
mod store;
mod templates;

pub use extract::{lookup, lookup_num, lookup_str, NumSpec};
pub use templates::{apply_template, templates, UsageTemplate};
pub(crate) use run::run;
pub(crate) use store::{delete as delete_config, load as load_config, load_enabled, save as save_config};

use serde::{Deserialize, Serialize};

use crate::fsutil::now_ms;

/// 一個來源的用量查詢設定（資料庫 `provider_usage_query` 一列）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageQueryConfig {
    pub provider_id: i64,
    pub enabled: bool,
    /// 樣板 id（`custom` = 全部自己填）
    pub template: String,
    /// 空 = 用來源自己的
    pub base_url: String,
    /// 空 = 用來源自己的
    pub api_key: String,
    pub access_token: String,
    pub user_id: String,
    /// 例：`{{baseUrl}}/user/balance`
    pub url_template: String,
    pub method: String,
    /// `[{"name":"Authorization","value":"Bearer {{apiKey}}"}]`
    pub headers: Vec<HeaderPair>,
    /// 抽取規格（見 `extract.rs`）
    pub extractor: serde_json::Value,
    pub timeout_secs: i64,
    /// 自動查詢間隔（分）；0 = 不自動查
    pub auto_interval_min: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

/// 多方案裡的一個方案。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PlanView {
    pub name: String,
    pub remaining: Option<f64>,
    pub used: Option<f64>,
    pub total: Option<f64>,
    pub unit: String,
}

/// 一次查詢的結果（給卡片顯示；**不含任何金鑰或 token**）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaView {
    pub provider_id: i64,
    /// 查得到且上游說有效
    pub ok: bool,
    /// 失敗原因／上游訊息（ok=false 時要看這個）
    pub message: String,
    pub unit: String,
    pub remaining: Option<f64>,
    pub used: Option<f64>,
    pub total: Option<f64>,
    /// 用量百分比（有 total 才算得出來；cc-switch 的顏色判斷用這個）
    pub percent: Option<f64>,
    pub plan_name: String,
    pub extra: String,
    pub plans: Vec<PlanView>,
    /// 原始回應（截斷保存；「測試」按鈕要看它才知道樣板對不對）
    pub raw: String,
    pub checked_at: i64,
}

impl QuotaView {
    pub fn failed(provider_id: i64, message: impl Into<String>) -> Self {
        Self {
            provider_id,
            ok: false,
            message: message.into(),
            checked_at: now_ms(),
            ..Default::default()
        }
    }
}

/// 把 `{{apiKey}}`、`{{baseUrl}}`、`{{accessToken}}`、`{{userId}}` 換成實際值。
///
/// 沒填的欄位換成空字串（與 cc-switch 的「多數情況可留空、沿用來源憑證」一致）。
pub fn expand(tpl: &str, cfg: &UsageQueryConfig, api_key: &str, base_url: &str) -> String {
    tpl.replace("{{apiKey}}", api_key.trim())
        .replace("{{baseUrl}}", base_url.trim())
        .replace("{{accessToken}}", cfg.access_token.trim())
        .replace("{{userId}}", cfg.user_id.trim())
}

/// 從回應 JSON 套用抽取規格。
///
/// 規則：
/// - `validPath`（可選）存在且為 false → 視為查詢失敗，用 `invalidMessagePath` 當訊息。
/// - 單一方案：`remaining`／`used`／`total`／`unit`／`planName`／`extra`。
/// - 多方案：`plans.path` 指到陣列，每項再用 `plans.<欄位>` 取。
/// - 三個數字都沒有 → `ok=false`（「有回應但取不到數字」跟「查詢失敗」要分得出來，
///   訊息會說「抽取規格沒對上回應」並附原始回應片段）。
pub fn apply_extractor(spec: &serde_json::Value, provider_id: i64, raw: &str) -> QuotaView {
    let mut view = QuotaView {
        provider_id,
        raw: truncate(raw, 2000),
        checked_at: now_ms(),
        ..Default::default()
    };
    let json: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            view.message = format!("回應不是 JSON（{e}）");
            return view;
        }
    };
    // 上游說這個帳號無效？
    if let Some(vp) = spec.get("validPath").and_then(|v| v.as_str()) {
        let valid = match lookup(&json, vp) {
            Some(serde_json::Value::Bool(b)) => *b,
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
            Some(serde_json::Value::String(s)) => {
                !matches!(s.trim().to_ascii_lowercase().as_str(), "" | "false" | "0")
            }
            // 路徑不存在時不當成「無效」（有的上游不給這個欄位）
            _ => true,
        };
        if !valid {
            view.message = spec
                .get("invalidMessagePath")
                .and_then(|v| v.as_str())
                .and_then(|p| lookup_str(&json, p))
                .unwrap_or_else(|| "上游回報帳號無效".to_string());
            return view;
        }
    }
    let num = |key: &str, root: &serde_json::Value| -> Option<f64> {
        spec.get(key).and_then(NumSpec::parse).and_then(|s| s.eval(root))
    };
    view.remaining = num("remaining", &json);
    view.used = num("used", &json);
    view.total = num("total", &json);
    view.unit = spec
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    view.plan_name = spec
        .get("planName")
        .and_then(|v| v.as_str())
        .and_then(|p| lookup_str(&json, p))
        .unwrap_or_default();
    view.extra = spec
        .get("extra")
        .and_then(|v| v.as_str())
        .and_then(|p| lookup_str(&json, p))
        .unwrap_or_default();

    // 多方案
    if let Some(plans_spec) = spec.get("plans") {
        if let Some(arr_path) = plans_spec.get("path").and_then(|v| v.as_str()) {
            if let Some(serde_json::Value::Array(items)) = lookup(&json, arr_path) {
                for it in items {
                    let get_str = |k: &str| -> String {
                        plans_spec
                            .get(k)
                            .and_then(|v| v.as_str())
                            .and_then(|p| lookup_str(it, p))
                            .unwrap_or_default()
                    };
                    let get_num = |k: &str| -> Option<f64> {
                        plans_spec
                            .get(k)
                            .and_then(|v| v.as_str())
                            .and_then(|p| lookup_num(it, p))
                    };
                    view.plans.push(PlanView {
                        name: get_str("name"),
                        remaining: get_num("remaining"),
                        used: get_num("used"),
                        total: get_num("total"),
                        unit: plans_spec
                            .get("unit")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| view.unit.clone()),
                    });
                }
            }
        }
    }

    // 百分比：優先 used/total，其次 (total-remaining)/total
    view.percent = match (view.used, view.total, view.remaining) {
        (Some(u), Some(t), _) if t > 0.0 => Some(clamp_pct(u / t * 100.0)),
        (None, Some(t), Some(r)) if t > 0.0 => Some(clamp_pct((t - r) / t * 100.0)),
        _ => None,
    };

    if view.remaining.is_none() && view.used.is_none() && view.total.is_none() && view.plans.is_empty()
    {
        view.message = "抽取規格沒對上回應（三個數字都取不到）：請用「測試」看原始回應，\
                        再改路徑（或換一個樣板）"
            .to_string();
        return view;
    }
    view.ok = true;
    view
}

fn clamp_pct(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 100.0)
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    fn cfg() -> UsageQueryConfig {
        UsageQueryConfig {
            provider_id: 7,
            access_token: "tok".into(),
            user_id: "42".into(),
            ..Default::default()
        }
    }

    #[test]
    fn expand_replaces_all_placeholders() {
        let c = cfg();
        let out = expand(
            "{{baseUrl}}/api/user/self?u={{userId}}",
            &c,
            "sk-abc",
            "https://relay.example.com",
        );
        assert_eq!(out, "https://relay.example.com/api/user/self?u=42");
        let h = expand("Bearer {{apiKey}}", &c, "sk-abc", "");
        assert_eq!(h, "Bearer sk-abc");
        // 沒填的欄位換成空字串（不是原樣留著讓網址帶著 {{...}} 出去）
        let empty = expand(
            "[{{apiKey}}][{{accessToken}}]",
            &UsageQueryConfig::default(),
            "",
            "",
        );
        assert_eq!(empty, "[][]");
        // 不認識的佔位符原樣保留（看得出來打錯字）
        assert_eq!(expand("{{nope}}", &c, "k", "b"), "{{nope}}");
    }

    #[test]
    fn newapi_shape_extracts_plan_and_balance() {
        // cc-switch 文件裡的 New API 範例（含 /500000 的單位換算）
        let raw = r#"{"success":true,"data":{"group":"預設方案","quota":500000,
                       "used_quota":250000},"message":"ok"}"#;
        let view = apply_extractor(
            &spec(
                r#"{"planName":"data.group","remaining":{"path":"data.quota","divide":500000},
                    "used":{"path":"data.used_quota","divide":500000},
                    "total":{"path":"data.quota","divide":500000},
                    "unit":"USD","validPath":"success","invalidMessagePath":"message"}"#,
            ),
            7,
            raw,
        );
        assert!(view.ok, "{}", view.message);
        assert_eq!(view.plan_name, "預設方案");
        assert_eq!(view.remaining, Some(1.0));
        assert_eq!(view.used, Some(0.5));
        assert_eq!(view.unit, "USD");
        assert_eq!(view.percent, Some(50.0));
        assert_eq!(view.provider_id, 7);
    }

    #[test]
    fn invalid_account_uses_upstream_message() {
        let raw = r#"{"success":false,"message":"Token 已過期"}"#;
        let view = apply_extractor(
            &spec(r#"{"remaining":"data.quota","validPath":"success","invalidMessagePath":"message"}"#),
            7,
            raw,
        );
        assert!(!view.ok);
        assert_eq!(view.message, "Token 已過期");
    }

    #[test]
    fn multi_plan_array_is_expanded() {
        let raw = r#"{"data":{"plans":[
            {"name":"5 小時","remaining":10,"total":40},
            {"name":"每週","remaining":50,"total":200}]}}"#;
        let view = apply_extractor(
            &spec(
                r#"{"plans":{"path":"data.plans","name":"name","remaining":"remaining",
                     "total":"total","unit":"次"}}"#,
            ),
            7,
            raw,
        );
        assert!(view.ok, "{}", view.message);
        assert_eq!(view.plans.len(), 2);
        assert_eq!(view.plans[0].name, "5 小時");
        assert_eq!(view.plans[0].remaining, Some(10.0));
        assert_eq!(view.plans[1].unit, "次");
        // 只有多方案、沒有單一方案時也要 ok（卡片會展開顯示）
        assert_eq!(view.remaining, None);
    }

    #[test]
    fn mismatched_spec_says_so_instead_of_pretending() {
        let view = apply_extractor(&spec(r#"{"remaining":"nope.balance"}"#), 7, r#"{"ok":1}"#);
        assert!(!view.ok);
        assert!(view.message.contains("抽取規格沒對上回應"), "{}", view.message);
        assert!(view.raw.contains("\"ok\""), "原始回應要留著給測試看");
    }

    #[test]
    fn non_json_response_is_reported() {
        let view = apply_extractor(&spec(r#"{"remaining":"balance"}"#), 7, "<html>502</html>");
        assert!(!view.ok);
        assert!(view.message.contains("不是 JSON"), "{}", view.message);
    }

    #[test]
    fn percent_uses_remaining_when_used_is_missing() {
        let view = apply_extractor(
            &spec(r#"{"remaining":"remain","total":"quota"}"#),
            7,
            r#"{"remain":25,"quota":100}"#,
        );
        assert_eq!(view.percent, Some(75.0));
    }
}
