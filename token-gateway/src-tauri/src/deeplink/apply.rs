//! 把 Deep Link 的內容真的寫進資料庫（P4.1）。
//!
//! 與解析分開是刻意的：解析只產生預覽，**使用者按下確認**才會走到這裡。
//! 每一種資源都沿用既有的 store 函式（單一寫入路徑，不另外開一條）。

use std::path::Path;

use rusqlite::Connection;

use super::parse::{ImportRequest, McpImport, PromptImport, ProviderImport, SkillImport};
use crate::prompt::PromptInput;
use crate::providers::ProviderInput;

/// 匯入結果（給 UI 顯示）。
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Applied {
    pub kind: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub note: String,
}

/// 套用一次匯入。`app_data` 只在「prompt 要求直接啟用」時用到（要寫工具檔案）。
///
/// 重複的判斷交給各 store（同名來源、同名的預設集／伺服器都會被跳過），
/// 這裡只負責組裝與回報。
pub fn apply(conn: &Connection, app_data: &Path, req: &ImportRequest) -> Result<Applied, String> {
    match req {
        ImportRequest::Provider(p) => apply_provider(conn, p),
        ImportRequest::Prompt(p) => apply_prompt(conn, app_data, p),
        ImportRequest::Mcp(m) => apply_mcp(conn, m),
        ImportRequest::Skill(s) => apply_skill(conn, s),
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

fn apply_prompt(conn: &Connection, app_data: &Path, p: &PromptImport) -> Result<Applied, String> {
    let mut out = Applied {
        kind: "prompt".into(),
        ..Default::default()
    };
    let existing = crate::prompt::list_presets(conn, &p.app)?
        .into_iter()
        .find(|x| x.name == p.name);
    let input = PromptInput {
        id: existing.as_ref().map(|x| x.id),
        app: p.app.clone(),
        name: p.name.clone(),
        content: p.content.clone(),
    };
    let saved = crate::prompt::save_preset(conn, &input)?;
    out.created.push(saved.name.clone());
    if p.enabled {
        // cc-switch 的 enabled＝「匯入即啟用」；我們照做（UI 的預覽會先警告會覆寫檔案）
        crate::prompt::activate_and_sync(conn, app_data, &p.app, saved.id)
            .map_err(|e| format!("已建立但啟用失敗：{e}"))?;
        out.note = "已建立並啟用（會寫進該工具的提示詞檔，改寫前已備份）".to_string();
    } else {
        out.note = "已建立（未啟用 —— 要啟用請到提示詞頁按）".to_string();
    }
    Ok(out)
}

fn apply_mcp(conn: &Connection, m: &McpImport) -> Result<Applied, String> {
    let mut out = Applied {
        kind: "mcp".into(),
        ..Default::default()
    };
    for (id, input) in &m.servers {
        let existing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mcp_servers WHERE slug=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if existing > 0 {
            out.skipped.push(id.clone());
            continue;
        }
        let saved = crate::mcp::upsert_server(conn, input)?;
        if m.enabled {
            for app in &m.apps {
                crate::mcp::set_binding(conn, saved.id, app, true)?;
            }
        }
        out.created.push(id.clone());
    }
    out.note = if out.created.is_empty() {
        "這些伺服器已經存在，沒有變動".to_string()
    } else {
        format!(
            "已新增 {} 個伺服器{}",
            out.created.len(),
            if m.enabled {
                "並同步到指定工具"
            } else {
                "（未啟用任何工具）"
            }
        )
    };
    Ok(out)
}

fn apply_skill(conn: &Connection, s: &SkillImport) -> Result<Applied, String> {
    let mut out = Applied {
        kind: "skill".into(),
        ..Default::default()
    };
    let (owner, name) = s
        .repo
        .split_once('/')
        .ok_or_else(|| format!("repo 格式應為 owner/name：{}", s.repo))?;
    let repo = crate::skills::add_repo(
        conn,
        owner,
        name,
        &s.branch,
        &s.directory,
        &s.name,
        false,
    )?;
    out.created.push(format!("{}（儲存庫 #{}）", s.name, repo.id));
    out.note = format!(
        "已新增儲存庫 {}/{}@{} —— 到「技能」頁按安裝即可下載（需要網路）",
        owner, name, s.branch
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deeplink::parse::{McpImport, PromptImport, ProviderImport, SkillImport};

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
    fn prompt_is_created_disabled_and_can_be_enabled() {
        let (_d, conn, app_data) = db();
        let req = ImportRequest::Prompt(PromptImport {
            app: "codex".into(),
            name: "共用提示".into(),
            content: "# 角色\n審查".into(),
            description: String::new(),
            enabled: false,
        });
        let out = apply(&conn, app_data.path(), &req).unwrap();
        assert_eq!(out.kind, "prompt");
        assert!(out.note.contains("未啟用"));
        let list = crate::prompt::list_presets(&conn, "codex").unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].active);
        assert_eq!(list[0].content, "# 角色\n審查");
    }

    #[test]
    fn mcp_servers_are_created_and_bound() {
        let (_d, conn, app_data) = db();
        let input = crate::mcp::input_from_json(
            "fetch",
            &serde_json::json!({"command": "uvx", "args": ["mcp-server-fetch"]}),
        )
        .unwrap();
        let req = ImportRequest::Mcp(McpImport {
            apps: vec!["claude".into()],
            enabled: true,
            servers: vec![("fetch".into(), input)],
        });
        let out = apply(&conn, app_data.path(), &req).unwrap();
        assert_eq!(out.created, vec!["fetch"]);
        let list = crate::mcp::list_servers(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].bindings.iter().any(|b| b.app == "claude" && b.enabled));
        // 重複匯入 → 跳過
        let again = apply(&conn, app_data.path(), &req).unwrap();
        assert!(again.created.is_empty() && again.skipped == vec!["fetch"]);
    }

    #[test]
    fn skill_adds_a_repo_for_later_install() {
        let (_d, conn, app_data) = db();
        let req = ImportRequest::Skill(SkillImport {
            repo: "owner/repo".into(),
            directory: "skills/my-skill".into(),
            branch: "main".into(),
            name: "my-skill".into(),
        });
        let out = apply(&conn, app_data.path(), &req).unwrap();
        assert_eq!(out.kind, "skill");
        assert!(out.note.contains("按安裝"));
        let repos = crate::skills::list_repos(&conn).unwrap();
        assert!(repos.iter().any(|r| r.owner == "owner" && r.name == "repo"));
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
