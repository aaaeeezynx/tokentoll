//! Phase 1：網關可觀測性（請求追蹤 + 上游拒收欄位持久化）。
//!
//! 背景（見 `docs/REFACTORING-PLAN.md` §5.2）：本專案原本面對「上游不認某欄位
//! 就回 400」的策略是**事後剝離重試**，這帶來兩個具體缺陷：
//!
//! 1. **記憶不持久**：`proxy.rs` 的 `ProxyCtx.stripped` 只是進程記憶體，
//!    網關每次停止／啟動就歸零 → 每個渠道的第一個請求都要重踩一次 400。
//! 2. **完全不可觀測**：剝離了哪些欄位、上游原文是什麼，只在錯誤時截斷 300 字
//!    透出，事後無法稽核。§5.2 那個無法判定根因的 `body_json_is_some=false`
//!    就是這個缺陷的直接後果。
//!
//! 本模組把兩件事落到 SQLite：
//! - [`remember_stripped`] / [`load_stripped`]：**持久化**的欄位拒收記憶。
//! - [`insert_trace`]：異常請求的完整上下文，含請求體指紋；
//!   body 解析失敗時額外記錄前 [`BODY_HEX_MAX`] bytes 的 hex，用來區分
//!   「真的解析失敗」與「debug 儀器自己弄壞了 body」。
//!
//! **設計約束**
//! - 只在異常時寫入（剝離／4xx／5xx／解析失敗），正常請求不寫，避免日誌洪水。
//! - 金鑰永不落庫：`body_hex` 只取前綴且由呼叫端在解析失敗時才帶入；
//!   正常路徑只存 SHA-256 指紋。
//! - 觀測設施**絕不可**影響請求處理：對外便利函式一律吞掉錯誤。
//!
//! 模組佈局：`record` 放追蹤層級與紀錄本體；`store` 放追蹤表讀寫；
//! `stripped` 放上游拒收欄位記憶；兩個子模組共用的常數與 hex/hash 小工具留在本檔；
//! 對外路徑 `crate::trace::*` 由下方 re-export 保持不變。

mod record;
mod store;
mod stripped;

// 保持原本 `crate::trace::*` 的取名路徑不變（部分條目僅測試使用，故允許未使用)。
#[allow(unused_imports)]
pub use record::*;
#[allow(unused_imports)]
pub use store::*;
#[allow(unused_imports)]
pub use stripped::*;

use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

/// body 解析失敗時最多記錄多少 bytes 的 hex（512 bytes → 1024 hex 字元）。
pub const BODY_HEX_MAX: usize = 512;

/// 上游錯誤原文入庫上限（存庫比對外顯示寬鬆，UI 端再截斷）。
pub const UPSTREAM_ERR_MAX: usize = 4000;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// 位元組級 SHA-256（`keys::sha256_hex` 只吃 `&str`）。
pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 取前綴 hex（每 byte 兩位小寫）。
pub fn hex_prefix(bytes: &[u8], max: usize) -> String {
    let n = bytes.len().min(max);
    let mut s = String::with_capacity(n * 2);
    for b in &bytes[..n] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
