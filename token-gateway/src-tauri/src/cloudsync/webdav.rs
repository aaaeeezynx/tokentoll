//! WebDAV 的實際操作（測試連線／上傳／下載）。
//!
//! 只用四個動詞，全部是標準 WebDAV：
//! - `PROPFIND`（Depth: 0）＝ 測試連線（207 Multi-Status 代表通）
//! - `MKCOL` ＝ 建立遠端目錄（已存在會回 405，那是正常的）
//! - `PUT` ＝ 上傳（資料庫檔 ＋ manifest）
//! - `GET` ＝ 下載（先抓 manifest 給使用者看，再抓資料庫）
//!
//! 認證用 Basic（WebDAV 的通用做法；Nextcloud／堅果雲這類都要用應用程式密碼）。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::store::WebdavConfig;
use super::{dir_url, DB_FILE, MANIFEST_FILE, PROTOCOL};

/// 遠端快照資訊（下載前顯示）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RemoteSnapshot {
    pub protocol: String,
    pub schema_version: i32,
    pub app_version: String,
    pub timestamp_ms: i64,
    pub bytes: u64,
    /// 協定版本不同 → UI 要警告
    pub incompatible: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncOutcome {
    pub action: String,
    pub bytes: u64,
    pub remote: String,
    pub note: String,
    pub snapshot: Option<RemoteSnapshot>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

fn req(
    c: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    cfg: &WebdavConfig,
) -> reqwest::RequestBuilder {
    c.request(method, url).basic_auth(&cfg.username, Some(&cfg.password))
}

/// 把 HTTP 狀態翻成人話（憑證錯、權限不足、路徑錯要分得出來）。
fn explain(status: reqwest::StatusCode, url: &str) -> String {
    match status.as_u16() {
        401 => "認證失敗（帳號或密碼不對；Nextcloud／堅果雲要填應用程式密碼）".to_string(),
        403 => "沒有權限（這個帳號不能寫入這個目錄）".to_string(),
        404 => format!("找不到路徑：{url}"),
        405 => "伺服器不接受這個動作（可能不支援 WebDAV）".to_string(),
        409 => "遠端目錄不存在（上層目錄要先建立）".to_string(),
        507 => "遠端空間不足".to_string(),
        c => format!("伺服器回 {c}（{url}）"),
    }
}

/// 測試連線。
pub async fn test_connection(cfg: &WebdavConfig) -> Result<String, String> {
    let c = client()?;
    let url = dir_url(&cfg.base_url, &cfg.remote_root, &cfg.profile);
    let r = req(&c, reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url, cfg)
        .header("Depth", "0")
        .send()
        .await
        .map_err(|e| format!("連線失敗：{e}"))?;
    let status = r.status();
    if status.is_success() || status.as_u16() == 207 || status.as_u16() == 404 {
        // 404 也算「連得上」—— 目錄還沒建立而已，上傳時會建
        Ok(format!(
            "連線成功（HTTP {}）；{}",
            status.as_u16(),
            if status.as_u16() == 404 {
                "遠端目錄尚未建立，上傳時會自動建立"
            } else {
                "遠端目錄存在"
            }
        ))
    } else if status.as_u16() == 401 {
        // 換一個一定會過的層級再確認一次：認證錯就是認證錯
        Err(explain(status, &url))
    } else {
        Err(explain(status, &url))
    }
}

async fn mkcol(c: &reqwest::Client, url: &str, cfg: &WebdavConfig) -> Result<(), String> {
    let r = req(c, reqwest::Method::from_bytes(b"MKCOL").unwrap(), url, cfg)
        .send()
        .await
        .map_err(|e| format!("建立遠端目錄失敗：{e}"))?;
    let s = r.status();
    // 201 建立成功、405 已經存在 —— 兩個都算成功
    if s.is_success() || s.as_u16() == 405 {
        Ok(())
    } else {
        Err(explain(s, url))
    }
}

/// 上傳（本地資料庫 → 遠端）。先建目錄，再 PUT 資料庫與 manifest。
pub async fn upload(
    cfg: &WebdavConfig,
    db_path: &std::path::Path,
    schema_version: i32,
    app_version: &str,
) -> Result<SyncOutcome, String> {
    let bytes = std::fs::read(db_path).map_err(|e| format!("讀取資料庫失敗：{e}"))?;
    let c = client()?;
    let root_url = format!(
        "{}/{}",
        cfg.base_url.trim_end_matches('/'),
        cfg.remote_root.trim_matches('/')
    );
    let dir = dir_url(&cfg.base_url, &cfg.remote_root, &cfg.profile);
    mkcol(&c, &root_url, cfg).await?;
    mkcol(&c, &dir, cfg).await?;

    let snapshot = RemoteSnapshot {
        protocol: PROTOCOL.to_string(),
        schema_version,
        app_version: app_version.to_string(),
        timestamp_ms: crate::fsutil::now_ms(),
        bytes: bytes.len() as u64,
        incompatible: false,
    };
    let manifest = serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?;

    let db_url = format!("{dir}/{DB_FILE}");
    let r = req(&c, reqwest::Method::PUT, &db_url, cfg)
        .body(bytes.clone())
        .send()
        .await
        .map_err(|e| format!("上傳失敗：{e}"))?;
    if !r.status().is_success() && r.status().as_u16() != 201 && r.status().as_u16() != 204 {
        return Err(explain(r.status(), &db_url));
    }
    let m_url = format!("{dir}/{MANIFEST_FILE}");
    let r2 = req(&c, reqwest::Method::PUT, &m_url, cfg)
        .body(manifest)
        .send()
        .await
        .map_err(|e| format!("寫入 manifest 失敗：{e}"))?;
    if !r2.status().is_success() && r2.status().as_u16() != 201 && r2.status().as_u16() != 204 {
        return Err(explain(r2.status(), &m_url));
    }

    Ok(SyncOutcome {
        action: "upload".to_string(),
        bytes: bytes.len() as u64,
        remote: db_url,
        note: format!("已上傳（{} bytes，涵蓋遠端舊資料）", bytes.len()),
        snapshot: Some(snapshot),
    })
}

/// 先看遠端快照（下載前顯示給使用者；不會動到本地）。
pub async fn remote_snapshot(cfg: &WebdavConfig) -> Result<RemoteSnapshot, String> {
    let c = client()?;
    let url = format!(
        "{}/{MANIFEST_FILE}",
        dir_url(&cfg.base_url, &cfg.remote_root, &cfg.profile)
    );
    let r = req(&c, reqwest::Method::GET, &url, cfg)
        .send()
        .await
        .map_err(|e| format!("讀取遠端資訊失敗：{e}"))?;
    if !r.status().is_success() {
        return Err(format!(
            "{}（這個設定檔還沒上傳過？）",
            explain(r.status(), &url)
        ));
    }
    let text = r.text().await.map_err(|e| e.to_string())?;
    let mut snap: RemoteSnapshot =
        serde_json::from_str(&text).map_err(|e| format!("遠端資訊格式不對：{e}"))?;
    snap.incompatible = snap.protocol != PROTOCOL;
    Ok(snap)
}

/// 抓遠端快照與資料庫位元組（**非同步、不碰本地資料庫**）。
///
/// 刻意與「寫回本地」分開：`rusqlite::Connection` 是 `Send` 但非 `Sync`，
/// 把鎖帶過 `.await` 會讓 Tauri 的 async 命令不成立（編譯器會說
/// future cannot be sent between threads safely）。所以下載分成
/// 「先抓」與「後套用」兩步，鎖只出現在第二步。
pub async fn fetch_remote_db(
    cfg: &WebdavConfig,
) -> Result<(RemoteSnapshot, Vec<u8>), String> {
    let snapshot = remote_snapshot(cfg).await?;
    if snapshot.incompatible {
        return Err(format!(
            "遠端協定版本不符（遠端 {}／本地 {PROTOCOL}）—— 請確認兩端 App 版本",
            snapshot.protocol
        ));
    }
    let c = client()?;
    let url = format!(
        "{}/{DB_FILE}",
        dir_url(&cfg.base_url, &cfg.remote_root, &cfg.profile)
    );
    let r = req(&c, reqwest::Method::GET, &url, cfg)
        .send()
        .await
        .map_err(|e| format!("下載失敗：{e}"))?;
    if !r.status().is_success() {
        return Err(explain(r.status(), &url));
    }
    let bytes = r.bytes().await.map_err(|e| e.to_string())?.to_vec();
    Ok((snapshot, bytes))
}

/// 把抓回來的內容套用到現有連線（**同步**）：先做本地安全備份，再用
/// SQLite 線上備份 API 寫回（與 `dbbackup::restore` 同一條路徑）。
pub fn apply_remote_db(
    conn: &mut Connection,
    app_data: &std::path::Path,
    snapshot: &RemoteSnapshot,
    bytes: &[u8],
) -> Result<SyncOutcome, String> {
    // ① 本地安全備份（cc-switch 也是下載前先備份）
    let safety = super::super::dbbackup::create(conn, app_data, None)?;
    // ② 落地到暫存檔再寫回現有連線
    let tmp = app_data.join("cloud-download.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("寫入暫存檔失敗：{e}"))?;
    let src = Connection::open(&tmp).map_err(|e| format!("開啟下載檔失敗：{e}"))?;
    {
        let backup =
            rusqlite::backup::Backup::new(&src, conn).map_err(|e| format!("還原下載內容失敗：{e}"))?;
        backup
            .run_to_completion(64, std::time::Duration::from_millis(5), None)
            .map_err(|e| format!("寫入下載內容失敗：{e}"))?;
    }
    drop(src);
    let _ = std::fs::remove_file(&tmp);
    let version: i32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap_or(0);
    if version <= 0 {
        return Err("下載的資料庫讀不到 schema 版本（檔案可能不完整）".to_string());
    }
    Ok(SyncOutcome {
        action: "download".to_string(),
        bytes: bytes.len() as u64,
        remote: String::new(),
        note: format!(
            "已下載並套用（schema v{version}）；下載前的本地資料庫已備份為 {}",
            safety.name
        ),
        snapshot: Some(snapshot.clone()),
    })
}
