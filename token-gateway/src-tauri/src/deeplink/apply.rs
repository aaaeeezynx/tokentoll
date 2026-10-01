//! 把 Deep Link 的內容真的寫進資料庫（P4.1）。
//!
//! 與解析分開是刻意的：解析只產生預覽，**使用者按下確認**才會走到這裡。
//! 每一種資源都沿用既有的 store 函式（單一寫入路徑，不另外開一條）。
//!
//! **只支援 `provider`**：`mcp` / `prompt` / `skill` 連同對應功能已於 2026-10-02 移除。

use std::path::Path;

use rusqlite::Connection;

use super::parse::{ImportRequest, ProviderImport};
use crate::providers::ProviderInput;

/// 匯入結果（給 UI 顯示）。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Applied {
    pub kind: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub note: String,
}

/// 套用一次匯入。
///
/// 重複的判斷交給各 store（同名來源會被跳過），這裡只負責組裝與回報。
/// `app_data` 保留在簽名裡是為了讓命令層不必因資源種類而分支（目前只有 provider
/// 會用到資料庫）；呼叫端仍照原樣傳入。
pub fn apply(conn: &Connection, _app_data: &Path, req: &ImportRequest) -> Result<Applied, String> {
    match req {
        ImportRequest::Provider(p) => apply_provider(conn, p),
    }
}

fn apply_provider(conn: &Connection, p: &ProviderImport) -> Result<Applied, String> {
    let mut out = Applied {
        kind: "provider".into(),
        ..Default::default()
    };
    if p.endpoints.is_empty() {
        return Err("連結沒有帶 endpoint，無法建立來源".to_string());
    }
    // 同名同網址已存在就跳過（不覆蓋使用者的設定）
    for ep in &p.endpoints {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM providers WHERE name=?1 AND base_url=?2",
                rusqlite::params![p.name, ep],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            out.skipped.push(ep.clone());
            continue;
        }
        let input = ProviderInput {
            name: p.name.clone(),
            app_type: "universal".to_string(),
            api_format: "openai-chat".to_string(),
            base_url: ep.clone(),
            api_key: p.api_key.clone(),
            auth_scheme: "bearer".to_string(),
            // 連結帶的 model 會一併登記成「可用模型」
            models: if p.model.trim().is_empty() {
                vec![]
            } else {
                vec![p.model.clone()]
            },
            priority: 0,
            enabled: p.enabled,
        };
        crate::providers::insert_provider(conn, &input, crate::fsutil::now_ms())?;
        // 連結帶的 model 也要真的登記成「可用模型」（UI 的模型清單讀這張表，
        // 只寫 providers.models_json 的話畫面上看不到）
        if !p.model.trim().is_empty() {
            let pid: i64 = conn
                .query_row(
                    "SELECT id FROM providers WHERE name=?1 AND base_url=?2 ORDER BY id DESC LIMIT 1",
                    rusqlite::params![p.name, ep],
                    |r| r.get(0),
                )
                .map_err(|e| format!("找不到剛建立的來源：{e}"))?;
            crate::models::model_add(
                conn,
                &crate::models::UsableModelInput {
                    provider_id: pid,
                    display_name: p.model.clone(),
                    actual_model: p.model.clone(),
                    context_window: None,
                    reasoning: String::new(),
                    ord: 0,
                    enabled: true,
                },
            )?;
        }
        out.created.push(ep.clone());
    }
    out.note = if out.created.is_empty() {
        "這個來源已經存在，沒有變動".to_string()
    } else {
        format!(
            "已新增 {} 個來源（協議預設 openai-chat，可在來源頁調整）",
            out.created.len()
        )
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deeplink::parse::ProviderImport;

    fn db() -> (tempfile::TempDir, Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_and_ensure(&dir.path().join("t.db")).unwrap();
        let app_data = tempfile::tempdir().unwrap();
        (dir, conn, app_data)
    }

    #[test]
    fn provider_is_created_with_model_and_key() {
        let (_d, conn, app_data) = db();
        let req = ImportRequest::Provider(ProviderImport {
            app: "codex".into(),
            name: "Shared".into(),
            endpoints: vec!["https://api.example.com".into()],
            api_key: "sk-shared".into(),
            model: "shared-model".into(),
            notes: String::new(),
            enabled: true,
        });
        let out = apply(&conn, app_data.path(), &req).unwrap();
        assert_eq!(out.created.len(), 1);
        let (base, key): (String, String) = conn
            .query_row(
                "SELECT base_url, api_key FROM providers WHERE name='Shared'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(base, "https://api.example.com");
        assert_eq!(key, "sk-shared");
        let models: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_models m
                 JOIN providers p ON p.id = m.provider_id
                 WHERE p.name='Shared'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(models, 1, "連結帶的 model 要登記成可用模型");

        // 再匯入一次 → 跳過（不覆蓋）
        let again = apply(&conn, app_data.path(), &req).unwrap();
        assert!(again.created.is_empty());
        assert_eq!(again.skipped.len(), 1);
        assert!(again.note.contains("已經存在"));
    }

    #[test]
    fn provider_without_endpoint_is_rejected() {
        let (_d, conn, app_data) = db();
        let req = ImportRequest::Provider(ProviderImport {
            app: "codex".into(),
            name: "x".into(),
            ..Default::default()
        });
        assert!(apply(&conn, app_data.path(), &req).unwrap_err().contains("endpoint"));
    }
}
