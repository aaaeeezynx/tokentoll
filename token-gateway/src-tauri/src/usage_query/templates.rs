//! 內建用量查詢樣板（P2.1，對標 cc-switch 的 built-in templates）。
//!
//! ## 收錄原則（與來源預設集同一條規矩）
//!
//! **只放講得出出處、而且使用者可以自己驗的樣板。** 每個樣板都附 `note`：
//! 它預期什麼形狀的回應、以及「先用測試確認」。猜錯的樣板比沒有樣板更糟 ——
//! 使用者會看到一個永遠查不到的卡片，卻不知道是網址錯還是欄位錯。
//!
//! 出處標記：
//! - `New API`：cc-switch 使用者手冊裡**原文**的範例（含 `/500000` 換算與
//!   `New-Api-User` 標頭），照抄。
//! - `通用餘額`：cc-switch 的 "Generic Template"（`{{baseUrl}}/user/balance`）。
//! - 其餘是各家公開文件常見的餘額端點；**形狀未經實測**（我們沒有那些金鑰），
//!   所以 note 會直說，並請使用者按「測試」看原始回應。

use serde::Serialize;

use super::UsageQueryConfig;

/// 一個樣板＝一組預填的設定（除了 provider_id）。
#[derive(Debug, Clone, Serialize)]
pub struct UsageTemplate {
    pub id: &'static str,
    pub name: &'static str,
    /// 這個樣板預期的回應形狀與注意事項
    pub note: &'static str,
    pub config: TemplateConfig,
}

/// 樣板的預填內容（欄位與 [`UsageQueryConfig`] 對應，但沒有 provider_id）。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct TemplateConfig {
    pub url_template: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub extractor: serde_json::Value,
    pub timeout_secs: i64,
    pub auto_interval_min: i64,
    /// 需要使用者自己填的欄位（前端要把它們標成必填）
    pub needs: Vec<String>,
}

const TABLES: &[(&str, &str, &str, &str, &str, &str, &str)] = &[
    // (id, name, url_template, method, headers_json, extractor_json, note)
    (
        "newapi",
        "New API 中轉",
        "{{baseUrl}}/api/user/self",
        "GET",
        r#"[["Content-Type","application/json"],["Authorization","Bearer {{accessToken}}"],["New-Api-User","{{userId}}"]]"#,
        r#"{"planName":"data.group","remaining":{"path":"data.quota","divide":500000},"used":{"path":"data.used_quota","divide":500000},"total":{"path":"data.quota","divide":500000},"unit":"USD","validPath":"success","invalidMessagePath":"message"}"#,
        "照抄 cc-switch 手冊的 New API 範例：回應是 {success, data:{group, quota, used_quota}}，quota 除以 500000 換成 USD。要填 Access Token 與 User ID。",
    ),
    (
        "generic-balance",
        "通用餘額（/user/balance）",
        "{{baseUrl}}/user/balance",
        "GET",
        r#"[["Authorization","Bearer {{apiKey}}"]]"#,
        r#"{"remaining":"balance","unit":"USD"}"#,
        "cc-switch 的 Generic Template：回應裡直接有 balance。若你的上游欄位名不同，改路徑或換樣板。",
    ),
    (
        "deepseek-balance",
        "DeepSeek 餘額",
        "{{baseUrl}}/user/balance",
        "GET",
        r#"[["Authorization","Bearer {{apiKey}}"]]"#,
        r#"{"remaining":"balance_infos.0.total_balance","unit":"CNY","validPath":"is_available","invalidMessagePath":"error.message"}"#,
        "DeepSeek 的 /user/balance 回 {is_available, balance_infos:[{currency,total_balance,…}]}。形狀取自公開文件、未經我方實測，請先按測試。",
    ),
    (
        "openrouter-credits",
        "OpenRouter 額度",
        "https://openrouter.ai/api/v1/credits",
        "GET",
        r#"[["Authorization","Bearer {{apiKey}}"]]"#,
        r#"{"remaining":{"path":"data.total_credits","subtract":"data.total_usage"},"used":"data.total_usage","total":"data.total_credits","unit":"USD"}"#,
        "OpenRouter 的 /credits 回 {data:{total_credits,total_usage}}；餘額＝總額−已用（樣板用 subtract）。網址寫死為官方端點。",
    ),
    (
        "siliconflow-balance",
        "硅基流動餘額",
        "{{baseUrl}}/user/info",
        "GET",
        r#"[["Authorization","Bearer {{apiKey}}"]]"#,
        r#"{"remaining":"data.totalBalance","used":"data.chargeBalance","unit":"CNY"}"#,
        "SiliconFlow 的 /user/info 回 {data:{totalBalance,chargeBalance,…}}。形狀取自公開文件、未經我方實測，請先按測試。",
    ),
    (
        "custom",
        "自訂",
        "",
        "GET",
        r#"[]"#,
        r#"{"remaining":"","unit":"USD"}"#,
        "全部自己填：網址支援 {{baseUrl}}/{{apiKey}}/{{accessToken}}/{{userId}} 佔位符；抽取規格可填路徑（balance_infos.0.total_balance）或帶運算的物件（{\"path\":\"quota\",\"divide\":500000}）。",
    ),
];

/// 全部樣板。
pub fn templates() -> Vec<UsageTemplate> {
    TABLES
        .iter()
        .map(|(id, name, url, method, headers, extractor, note)| UsageTemplate {
            id,
            name,
            note,
            config: TemplateConfig {
                url_template: url.to_string(),
                method: method.to_string(),
                headers: serde_json::from_str::<Vec<(String, String)>>(headers)
                    .unwrap_or_default(),
                extractor: serde_json::from_str(extractor)
                    .unwrap_or(serde_json::Value::Null),
                timeout_secs: 10,
                auto_interval_min: 0,
                needs: needs_for(id),
            },
        })
        .collect()
}

/// 這個樣板需要使用者自己填哪些欄位（空 = 直接用來源的憑證）。
fn needs_for(id: &str) -> Vec<String> {
    match id {
        "newapi" => vec!["access_token".into(), "user_id".into()],
        "openrouter-credits" => vec!["api_key".into()],
        // 其餘用來源的 base_url ＋ api_key（多數情況可留空）
        _ => vec![],
    }
}

/// 依 id 取樣板。
pub fn template(id: &str) -> Option<UsageTemplate> {
    templates().into_iter().find(|t| t.id == id)
}

/// 把樣板套用到一份設定上（保留 provider_id 與 enabled）。
///
/// `base_url`／`api_key` 刻意**不覆蓋**：使用者填過的憑證要留著，
/// 樣板只決定「查哪裡、怎麼抽」。
pub fn apply_template(mut cfg: UsageQueryConfig, tpl_id: &str) -> UsageQueryConfig {
    let Some(t) = template(tpl_id) else {
        return cfg;
    };
    cfg.template = t.id.to_string();
    cfg.url_template = t.config.url_template;
    cfg.method = t.config.method;
    cfg.headers = t
        .config
        .headers
        .into_iter()
        .map(|(name, value)| super::HeaderPair { name, value })
        .collect();
    cfg.extractor = t.config.extractor;
    cfg.timeout_secs = t.config.timeout_secs;
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_ids_are_unique_and_well_formed() {
        let list = templates();
        assert!(list.len() >= 5);
        let mut ids: Vec<&str> = list.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "樣板 id 有重複");
        for t in &list {
            assert!(!t.name.is_empty(), "{}：名稱不可為空", t.id);
            assert!(!t.note.is_empty(), "{}：說明不可為空（要講清楚預期形狀）", t.id);
            assert!(!t.config.method.is_empty());
            assert!(t.config.timeout_secs > 0);
            assert!(t.config.extractor.is_object(), "{}：抽取規格必須是物件", t.id);
            // 每個樣板（除了 custom）都要有網址
            if t.id != "custom" {
                assert!(!t.config.url_template.is_empty(), "{}：缺網址", t.id);
            }
        }
    }

    /// 照抄 cc-switch 的那個範例必須**逐欄**正確：這是我們唯一有原文出處的樣板。
    #[test]
    fn newapi_template_matches_the_documented_example() {
        let t = template("newapi").unwrap();
        assert_eq!(t.config.url_template, "{{baseUrl}}/api/user/self");
        assert_eq!(t.config.method, "GET");
        let hdr: Vec<(String, String)> = t
            .config
            .headers
            .iter()
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect();
        assert!(hdr.iter().any(|(n, v)| n == "Authorization" && v == "Bearer {{accessToken}}"));
        assert!(hdr.iter().any(|(n, v)| n == "New-Api-User" && v == "{{userId}}"));
        assert_eq!(t.config.extractor["planName"], "data.group");
        assert_eq!(t.config.extractor["remaining"]["divide"], 500000.0);
        assert_eq!(t.config.extractor["unit"], "USD");
        assert_eq!(t.config.needs, vec!["access_token", "user_id"]);
    }

    #[test]
    fn apply_template_keeps_credentials() {
        let cfg = UsageQueryConfig {
            provider_id: 7,
            enabled: true,
            base_url: "https://relay.example.com".into(),
            api_key: "sk-keep".into(),
            access_token: "tok-keep".into(),
            user_id: "9".into(),
            ..Default::default()
        };
        let out = apply_template(cfg, "newapi");
        assert_eq!(out.provider_id, 7);
        assert!(out.enabled, "套樣板不該把開關關掉");
        assert_eq!(out.base_url, "https://relay.example.com", "憑證要保留");
        assert_eq!(out.api_key, "sk-keep");
        assert_eq!(out.access_token, "tok-keep");
        assert_eq!(out.template, "newapi");
        assert_eq!(out.url_template, "{{baseUrl}}/api/user/self");
        assert!(out.extractor.is_object());
        // 未知樣板 id：原樣返回（不亂改使用者設定）
        let same = apply_template(
            UsageQueryConfig {
                template: "mine".into(),
                ..Default::default()
            },
            "nope",
        );
        assert_eq!(same.template, "mine");
    }
}
