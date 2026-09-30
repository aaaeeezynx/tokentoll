//! 用量查詢命令（P2.1，對標 cc-switch 的 Usage Query 面板）。
//!
//! 面板要的四件事：讀設定、存設定、套樣板、**測試**（真的打一次）。
//! 另有「卡片查詢」：對已啟用的來源跑一次並回傳顯示用資料。
//!
//! 憑證處理原則與其他模組一致：**設定可以回前端**（使用者要編輯它），但查詢結果
//! `QuotaView` 只帶數字與原始回應，**不帶金鑰**；原始回應截斷保存，方便使用者
//! 看出「樣板對不上」還是「上游真的沒回數字」。

use tauri::State;

use crate::db::DbState;
use crate::usage_query::{
    self, QuotaView, UsageQueryConfig, UsageTemplate,
};

/// 來源的連線資訊（查詢時要沿用）。
struct ProviderRow {
    name: String,
    base_url: String,
    api_key: String,
}

fn provider_row(db: &State<DbState>, provider_id: i64) -> Result<ProviderRow, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT name, base_url, api_key FROM providers WHERE id=?1",
        [provider_id],
        |r| {
            Ok(ProviderRow {
                name: r.get(0)?,
                base_url: r.get(1)?,
                api_key: r.get(2)?,
            })
        },
    )
    .map_err(|e| format!("來源不存在（{provider_id}）：{e}"))
}

/// 讀一份設定；沒設定過就回一份空白預設（前端直接編輯）。
#[tauri::command]
pub fn usage_query_get(db: State<DbState>, provider_id: i64) -> Result<UsageQueryConfig, String> {
    let row = provider_row(&db, provider_id)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    Ok(usage_query::load_config(&conn, provider_id).unwrap_or(UsageQueryConfig {
        provider_id,
        enabled: false,
        template: "custom".to_string(),
        // 預設沿用來源的憑證（cc-switch 的多數情況也是留空＝沿用）
        base_url: row.base_url,
        api_key: row.api_key,
        method: "GET".to_string(),
        timeout_secs: 10,
        ..Default::default()
    }))
}

/// 存設定。空白的 base_url／api_key 會被填成來源自己的值 ——
/// 這樣「留空＝沿用來源」在資料庫裡是明確的，不必每次查詢再推導一次。
#[tauri::command]
pub fn usage_query_set(
    db: State<DbState>,
    config: UsageQueryConfig,
) -> Result<UsageQueryConfig, String> {
    let row = provider_row(&db, config.provider_id)?;
    let mut cfg = config;
    if cfg.base_url.trim().is_empty() {
        cfg.base_url = row.base_url;
    }
    if cfg.api_key.trim().is_empty() {
        cfg.api_key = row.api_key;
    }
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage_query::save_config(&conn, &cfg)?;
    Ok(usage_query::load_config(&conn, cfg.provider_id).unwrap_or(cfg))
}

/// 關掉並清掉設定（回到「沒設定過」）。
#[tauri::command]
pub fn usage_query_clear(db: State<DbState>, provider_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    usage_query::delete_config(&conn, provider_id).map(|_| ())
}

/// 全部內建樣板（面板的選單）。
#[tauri::command]
pub fn usage_query_templates() -> Vec<UsageTemplate> {
    usage_query::templates()
}

/// 把樣板套到設定上（不回寫資料庫；前端接著顯示、按儲存才落庫）。
#[tauri::command]
pub fn usage_query_apply_template(
    db: State<DbState>,
    provider_id: i64,
    template: String,
) -> Result<UsageQueryConfig, String> {
    let row = provider_row(&db, provider_id)?;
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let cur = usage_query::load_config(&conn, provider_id).unwrap_or(UsageQueryConfig {
        provider_id,
        base_url: row.base_url.clone(),
        api_key: row.api_key.clone(),
        method: "GET".into(),
        timeout_secs: 10,
        ..Default::default()
    });
    Ok(usage_query::apply_template(cur, &template))
}

/// 跑一次查詢（面板的「測試」與卡片的「重新查詢」共用）。
///
/// `save_first`：測試時前端會先把面板上的設定存起來再跑，避免「測到的」與
/// 「存起來的」不一樣。這裡不做隱式儲存 —— 命令語意保持單純。
#[tauri::command]
pub async fn usage_query_run(
    db: State<'_, DbState>,
    provider_id: i64,
) -> Result<QuotaView, String> {
    let (cfg, provider) = {
        let row = provider_row(&db, provider_id)?;
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let cfg = usage_query::load_config(&conn, provider_id)
            .ok_or_else(|| format!("來源「{}」還沒設定用量查詢", row.name))?;
        (cfg, row)
    };
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;
    // 設定裡的憑證優先（使用者可能在面板填了專用的 token）
    let api_key = if cfg.api_key.trim().is_empty() {
        provider.api_key.clone()
    } else {
        cfg.api_key.clone()
    };
    let base_url = if cfg.base_url.trim().is_empty() {
        provider.base_url.clone()
    } else {
        cfg.base_url.clone()
    };
    Ok(usage_query::run(&client, &cfg, &api_key, &base_url).await)
}

/// 所有**已啟用**設定的來源，各跑一次查詢（來源頁卡片用）。
///
/// **刻意循序執行**：並行打多個上游只會讓「查額度」這件事變得更像攻擊，
/// 而且 cc-switch 的自動查詢本來就只針對「目前使用中」的那一個來源（省額度）。
/// 這裡由前端決定何時呼叫（來源頁開啟時、或按重新查詢）。
#[tauri::command]
pub async fn usage_query_run_all(
    db: State<'_, DbState>,
) -> Result<Vec<QuotaView>, String> {
    let rows: Vec<(UsageQueryConfig, String, String)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        usage_query::load_enabled(&conn)
            .into_iter()
            .map(|cfg| {
                let (base, key) = conn
                    .query_row(
                        "SELECT base_url, api_key FROM providers WHERE id=?1",
                        [cfg.provider_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                    )
                    .unwrap_or_default();
                (cfg, base, key)
            })
            .collect()
    };
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(6))
        .build()
    else {
        return Ok(vec![]);
    };
    let mut out = Vec::with_capacity(rows.len());
    for (cfg, base, key) in rows {
        let api_key = if cfg.api_key.trim().is_empty() {
            key
        } else {
            cfg.api_key.clone()
        };
        let base_url = if cfg.base_url.trim().is_empty() {
            base
        } else {
            cfg.base_url.clone()
        };
        out.push(usage_query::run(&client, &cfg, &api_key, &base_url).await);
    }
    Ok(out)
}
