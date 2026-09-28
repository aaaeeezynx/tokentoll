//! Codex 接管前體檢。

use super::super::*;
use super::legacy::{codex_legacy_providers_report, dedupe_codex_sections, LegacyProviders};

/// Codex 接管前體檢：返回逐行結論（✅ 正常 / ⚠️ 接管時處理 / ❌ 先手工排除）。
/// 純讀取，不寫任何檔案。
pub fn codex_doctor_at(codex_home: &Path, port: u16) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let gw = format!("http://{GATEWAY_HOST}:{port}/v1");
    let cfg_path = codex_home.join("config.toml");
    match std::fs::read_to_string(&cfg_path) {
        Err(_) => out.push("⚠️ config.toml 不存在：接管將新建".to_string()),
        Ok(t) => {
            for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                let header = format!("[model_providers.{id}]");
                let n = t.lines().filter(|l| l.trim() == header).count();
                if n > 1 {
                    out.push(format!(
                        "❌ {header} 重複 {n} 次（非法 TOML）：接管入口將自動去重消毒"
                    ));
                }
            }
            match dedupe_codex_sections(&t).parse::<toml_edit::DocumentMut>() {
                Err(e) => out.push(format!("❌ config.toml 解析失敗：{e}")),
                Ok(doc) => {
                    if doc.get("model_provider").and_then(|v| v.as_str())
                        != Some(CODEX_SHARED_PROVIDER_ID)
                    {
                        out.push("⚠️ model_provider 不是 custom：接管將切到共享段名".to_string());
                    }
                    for id in [CODEX_SHARED_PROVIDER_ID, GATEWAY_PROVIDER_ID] {
                        match doc
                            .get("model_providers")
                            .and_then(|m| m.get(id))
                            .and_then(|s| s.get("base_url"))
                            .and_then(|u| u.as_str())
                        {
                            Some(u) if u == gw => {}
                            Some(u) => out.push(format!(
                                "⚠️ [model_providers.{id}] base_url = {u}，不是本網關 {gw}（接管時將覆寫為網關；僅提示，不影響開關按鈕）"
                            )),
                            None => {
                                out.push(format!("⚠️ [model_providers.{id}] 缺失：接管將補建"))
                            }
                        }
                    }
                    match doc.get("model_catalog_json").and_then(|v| v.as_str()) {
                        Some(c) if !std::path::Path::new(c).exists() => {
                            out.push(format!("⚠️ model_catalog_json 指向缺失：{c}（接管將重寫）"))
                        }
                        None => out.push("⚠️ 未設 model_catalog_json：接管將按來源生成".to_string()),
                        _ => {}
                    }
                }
            }
        }
    }
    // **這裡以前會把讀取失敗報成 ✅。** 讀不到時 `codex_legacy_providers()`
    // 回傳空 vec，於是體檢顯示「歷史會話無第三方 provider 殘留」——一個失敗
    // 被當成通過，使用者唯一的診斷工具反而誤導他。現在失敗一律 ❌ 並附原因。
    match codex_legacy_providers_report(codex_home) {
        LegacyProviders::Failed { reason } => out.push(format!(
            "❌ 讀不到 Codex 歷史會話的 provider 名：{reason}。接管將無法補寫別名段，\
             仍在使用舊會話的 provider 可能失效 —— 請完全結束 Codex 後重試"
        )),
        LegacyProviders::Ok { providers, db } => {
            let src = db.file_name().unwrap_or_default().to_string_lossy().to_string();
            if providers.is_empty() {
                out.push(format!(
                    "✅ 歷史會話無第三方 provider 殘留（custom / tokengateway 之外；讀自 {src}）"
                ));
            } else {
                out.push(format!(
                    "ℹ️ 歷史會話用過 {} 個別名 provider（讀自 {src}）：{} —— 接管將全寫為網關別名段",
                    providers.len(),
                    providers.join(", ")
                ));
            }
        }
    }
    if std::env::var(GATEWAY_ENV_KEY)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        out.push(format!("✅ {GATEWAY_ENV_KEY} 已在環境變數"));
    } else {
        out.push(format!(
            "❌ {GATEWAY_ENV_KEY} 未設定：Codex 行程繼承不到 Key 會 401，請先設為用戶環境變數"
        ));
    }
    // cc-switch 代理殘留（15721）會與接管預期衝突
    let probe: std::net::SocketAddr = format!("{GATEWAY_HOST}:15721").parse().unwrap();
    if std::net::TcpStream::connect_timeout(&probe, std::time::Duration::from_millis(300)).is_ok()
    {
        out.push("❌ 15721 被佔用（疑似 cc-switch 代理仍在跑）：先完全退出 cc-switch 再接管".to_string());
    } else {
        out.push("✅ 15721 空閒（cc-switch 代理未運行）".to_string());
    }
    out
}


/// Codex 接管前體檢（用戶主目錄版）。
pub fn codex_doctor(port: u16) -> Vec<String> {
    match user_home() {
        Ok(h) => codex_doctor_at(&h.join(".codex"), port),
        Err(e) => vec![format!("❌ 用戶目錄不可用：{e}")],
    }
}
