//! GitHub 存取（P3.3）：掃描儲存庫裡的技能、下載技能資料夾。
//!
//! 兩條路徑：
//! - **掃描**用 contents API（一次請求列出子目錄，判斷哪些是技能＝有 `SKILL.md`）。
//! - **安裝／更新**用 **tarball**（`codeload.github.com`，一次請求拿到整個 repo），
//!   再從裡面挑出那個技能資料夾。tarball 比逐檔抓省得多，也不會撞到匿名 API
//!   每小時 60 次的限制。
//!
//! 兩者都不需要 token（公開儲存庫）；若之後要支援私有庫，再加 Authorization 標頭。

use serde::Deserialize;

use super::{RemoteSkill, SkillRepo};

/// 一次 HTTP 抓取的結果（body + 是否 404 —— 404 要能與其他錯誤分開講）。
pub(crate) enum Fetch {
    Ok(Vec<u8>),
    NotFound,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

fn api_base(repo: &SkillRepo) -> String {
    format!("https://api.github.com/repos/{}/{}", repo.owner, repo.name)
}

/// contents API 的目錄項。
#[derive(Debug, Deserialize)]
struct ContentEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

/// tarball 網址（`refs/heads/{branch}` 對分支最穩）。
pub(crate) fn tarball_url(repo: &SkillRepo) -> String {
    format!(
        "https://codeload.github.com/{}/{}/tar.gz/refs/heads/{}",
        repo.owner, repo.name, repo.branch
    )
}

async fn get(client: &reqwest::Client, url: &str, accept: &str) -> Result<Fetch, String> {
    let r = client
        .get(url)
        .header("accept", accept)
        .header("user-agent", "token-gateway-skills")
        .send()
        .await
        .map_err(|e| format!("連線 GitHub 失敗：{e}"))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Fetch::NotFound);
    }
    if !r.status().is_success() {
        return Err(format!(
            "GitHub 回 {}（{}）",
            r.status().as_u16(),
            url
        ));
    }
    Ok(Fetch::Ok(
        r.bytes().await.map_err(|e| e.to_string())?.to_vec(),
    ))
}

/// 列出某個儲存庫裡的技能（＝子目錄，且裡面有 `SKILL.md`）。
///
/// 判斷「有沒有 SKILL.md」要再各打一次 contents API；為了省請求，
/// 先列出目錄清單（1 次），再**只在需要時**檢查（安裝時 tarball 裡就有）。
/// 這裡採用的規則與 Claude 技能慣例一致：`subdir` 底下的每個子目錄都是一個技能。
pub(crate) async fn list_skills(repo: &SkillRepo) -> Result<Vec<RemoteSkill>, String> {
    let url = if repo.subdir.trim().is_empty() {
        format!("{}/contents/", api_base(repo))
    } else {
        format!("{}/contents/{}", api_base(repo), repo.subdir.trim())
    };
    let client = client()?;
    let body = match get(&client, &url, "application/vnd.github+json").await? {
        Fetch::Ok(b) => b,
        Fetch::NotFound => {
            return Err(format!(
                "找不到路徑（{}/{} 的 {}）—— 檢查儲存庫設定",
                repo.owner,
                repo.name,
                if repo.subdir.is_empty() {
                    "根目錄"
                } else {
                    &repo.subdir
                }
            ))
        }
    };
    let entries: Vec<ContentEntry> =
        serde_json::from_slice(&body).map_err(|e| format!("GitHub 回應不是預期的 JSON：{e}"))?;
    Ok(entries
        .into_iter()
        .filter(|e| e.kind == "dir")
        .map(|e| RemoteSkill {
            name: e.name,
            path: e.path,
            repo_id: repo.id,
            repo_label: repo_label(repo),
            installed: false,
            update_available: false,
        })
        .collect())
}

fn repo_label(repo: &SkillRepo) -> String {
    if repo.label.trim().is_empty() {
        format!("{}/{}", repo.owner, repo.name)
    } else {
        repo.label.clone()
    }
}

/// 下載整個儲存庫的 tarball，挑出某個技能資料夾的所有檔案。
///
/// 回傳 `(相對於技能資料夾的路徑, 內容)`。**至少要有 `SKILL.md`**，
/// 否則回錯誤 —— 那代表掃描到的東西其實不是技能。
pub(crate) async fn fetch_skill(
    repo: &SkillRepo,
    skill_path: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let client = client()?;
    let url = tarball_url(repo);
    let raw = match get(&client, &url, "application/octet-stream").await? {
        Fetch::Ok(b) => b,
        Fetch::NotFound => {
            return Err(format!(
                "找不到儲存庫或分支（{}/{} @ {}）",
                repo.owner, repo.name, repo.branch
            ))
        }
    };
    let files = super::tarball::read_tar_gz(&raw)?;
    let prefix = format!("{}/", skill_path.trim_end_matches('/'));
    let picked: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .filter_map(|(p, d)| p.strip_prefix(&prefix).map(|r| (r.to_string(), d)))
        .filter(|(r, _)| !r.is_empty())
        .collect();
    if picked.is_empty() {
        return Err(format!(
            "tarball 裡找不到 {}（儲存庫 {}/{}）",
            skill_path, repo.owner, repo.name
        ));
    }
    if !picked.iter().any(|(p, _)| p == "SKILL.md") {
        return Err(format!(
            "{} 裡沒有 SKILL.md —— 這個資料夾不是技能",
            skill_path
        ));
    }
    Ok(picked)
}

/// 便利函式：掃描多個儲存庫（單一儲存庫失敗不影響其他）。
pub async fn list_all(
    repos: &[SkillRepo],
) -> (Vec<RemoteSkill>, Vec<(String, String)>) {
    let mut skills = vec![];
    let mut errors = vec![];
    for r in repos {
        match list_skills(r).await {
            Ok(mut list) => skills.append(&mut list),
            Err(e) => errors.push((repo_label(r), e)),
        }
    }
    (skills, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> SkillRepo {
        SkillRepo {
            id: 1,
            owner: "anthropics".into(),
            name: "skills".into(),
            branch: "main".into(),
            subdir: "skills".into(),
            label: "Anthropic 官方技能".into(),
            builtin: true,
        }
    }

    /// **真的打 GitHub**：列出 `anthropics/skills` 的技能，確認我們認得它的版面配置。
    ///
    /// 這條測試會用到網路；沒有網路時會失敗（在 CI 可以標 ignore），
    /// 但這是「不憑想像」的代價 —— 版面配置一旦改了就該知道。
    #[tokio::test]
    #[ignore = "需要網路：實際打 GitHub API"]
    async fn lists_the_anthropic_repo() {
        let list = list_skills(&repo()).await.unwrap();
        assert!(!list.is_empty(), "應該掃到技能");
        assert!(
            list.iter().any(|s| s.name == "claude-api"),
            "預期看到 claude-api：{:?}",
            list.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        let s = list.iter().find(|s| s.name == "claude-api").unwrap();
        assert_eq!(s.path, "skills/claude-api");
        assert_eq!(s.repo_label, "Anthropic 官方技能");
    }

    /// 掃描一個不存在的儲存庫 → 明確錯誤，不是空的成功。
    #[tokio::test]
    #[ignore = "需要網路"]
    async fn missing_repo_is_an_error() {
        let mut r = repo();
        r.name = "definitely-not-a-real-repo-xyz".into();
        let e = list_skills(&r).await.unwrap_err();
        assert!(e.contains("找不到") || e.contains("404"), "{e}");
    }

    /// 下載 tarball 並挑出技能資料夾：要有 SKILL.md，且路徑相對於技能資料夾。
    #[tokio::test]
    #[ignore = "需要網路：會下載整個 repo 的 tarball"]
    async fn fetches_a_skill_folder() {
        let files = fetch_skill(&repo(), "skills/claude-api").await.unwrap();
        assert!(files.iter().any(|(p, _)| p == "SKILL.md"), "要有 SKILL.md");
        assert!(
            files.iter().any(|(p, _)| p.starts_with("python/")),
            "skill 內部的子目錄要一起帶走：{:?}",
            files.iter().map(|(p, _)| p).take(5).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tarball_url_shape() {
        assert_eq!(
            tarball_url(&repo()),
            "https://codeload.github.com/anthropics/skills/tar.gz/refs/heads/main"
        );
    }

    #[test]
    fn label_falls_back_to_owner_name() {
        let mut r = repo();
        r.label = String::new();
        assert_eq!(repo_label(&r), "anthropics/skills");
    }
}
