//! M6：模型目錄抓取 + 可用模型映射（對標 cc-switch 模型映射）。
//!
//! - 一鍵抓取上游 `/models`（OpenAI 兼容 `{data[]}` / Gemini `{models[]}`），緩存 ID 列表。
//! - 可用模型表：選單顯示名 / 實際請求模型 / 上下文視窗 / 思考等級 / 啟用。
//! - Codex 模型目錄：模板複製用户現有 catalog（版本安全），只改 slug/顯示名/
//!   上下文/思考等級，其餘上百欄位原樣保留。
//!
//! 本模組原本是單一檔案（990 行），現依主題拆成子模組：
//! - `catalog`：上游 `/models` 抓取、解析與目錄快取
//! - `crud`：可用模型 CRUD、思考等級白名單與 `resolve_actual`
//! - `codex_catalog`：Codex 模型目錄生成
//! - `tests`：單元測試（僅測試組建）
//!
//! `crate::models::X` 這個路徑對所有對外名稱都不變（`REASONINGS`、
//! `CatalogEntry`、`CatalogCache`、`UsableModel`、`UsableModelInput`、
//! `catalog_get`、`catalog_read_cfg`、`catalog_fetch_http`、`catalog_write`、
//! `models_list`、`model_counts`、`model_add`、`model_update`、`model_delete`、
//! `resolve_actual`、`codex_catalog_generate`）。

mod catalog;
mod codex_catalog;
mod crud;

#[cfg(test)]
mod tests;

pub use catalog::*;
pub(crate) use codex_catalog::*;
pub use crud::*;
