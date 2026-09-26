//! 具型別的命令錯誤（Phase 5）。
//!
//! ## 為什麼要做這件事
//!
//! 目前全 crate 有 108 處 `Result<_, String>`。這有兩個實際問題：
//!
//! 1. **無法區分類別。** 呼叫端拿到一個 `String` 無從判斷「這是使用者輸入
//!    錯誤（該顯示在欄位旁）」還是「這是系統故障（該顯示為紅色橫幅）」。
//! 2. **無法測試。** 測試只能 `assert!(x.is_err())` 或比對整串訊息，訊息一改
//!    測試就紅 —— 那不是測行為，是測文案。
//!
//! ## 為什麼不全面替換
//!
//! 108 處的錯誤訊息**就是 UI 契約**：前端到處是
//! `onError: (e) => setErr(String(e))`，顯示的就是這串文字。全面改成
//! enum 會同時改動 108 個呼叫點與所有顯示文案，風險遠大於收益。
//!
//! 因此採取**漸進**路線：新增 `CmdError`，讓「訊息」保持逐字不變（
//! `Display` 輸出與原本的 `String` 完全相同），另外附上**類別**。既有
//! `Result<_, String>` 不必一次改完，可以在各自模組被碰到時逐步換過來，
//! 過程中 UI 行為不變。
//!
//! ## 什麼時候該用
//!
//! 新增的命令錯誤請用 `CmdError`；既有程式碼維持原狀直到它被改動。

use std::fmt;

/// 命令層錯誤的類別。
///
/// 類別決定 UI 該怎麼呈現，這也是引入型別的主要理由 —— 光靠訊息文字
/// 判斷類別是脆弱的（改一個字就可能讓判斷失效）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrKind {
    /// 使用者輸入不合法（缺欄位、格式錯、超範圍）。
    /// UI：顯示在對應欄位旁。
    Invalid,
    /// 找不到目標（來源、Key、模型、備份）。
    /// UI：提示重新選擇，通常可自行恢復。
    NotFound,
    /// 狀態衝突（已停用、已存在、已過期、名額用完）。
    /// UI：說明現況與可行動建議。
    Conflict,
    /// 認證失敗（Key 無效、未設定）。
    /// UI：引導去設定憑證。
    Auth,
    /// 系統／IO／資料庫錯誤（非使用者造成）。
    /// UI：紅色橫幅，附上原始訊息以便回報。
    Internal,
}

impl ErrKind {
    /// 穩定的機器可讀代碼（給前端做 i18n 或分支判斷用）。
    pub fn code(self) -> &'static str {
        match self {
            ErrKind::Invalid => "invalid",
            ErrKind::NotFound => "not_found",
            ErrKind::Conflict => "conflict",
            ErrKind::Auth => "auth",
            ErrKind::Internal => "internal",
        }
    }
}

/// 具型別的命令錯誤。
///
/// `Display` **逐字**輸出原本的訊息，因此在 UI 上與 `String` 版本不可區分 ——
/// 這是刻意的：轉換過程不需要同步改動任何顯示邏輯。
#[derive(Debug, Clone)]
pub struct CmdError {
    kind: ErrKind,
    message: String,
}

impl CmdError {
    pub fn new(kind: ErrKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrKind::Invalid, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrKind::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrKind::Conflict, message)
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(ErrKind::Auth, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrKind::Internal, message)
    }

    pub fn kind(&self) -> ErrKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// 拆出訊息，讓呼叫端在需要 `String` 的介面（例如 Tauri 命令回傳值）
    /// 邊界上取得原本的文字。
    pub fn into_message(self) -> String {
        self.message
    }

    /// 包裝任何 `Display` 錯誤為 `Internal`（IO、SQLite、serde 等）。
    ///
    /// 這些錯誤一律不是使用者造成的，訊息保留原文以便回報。
    pub fn wrap(context: &str, e: impl fmt::Display) -> Self {
        Self::internal(format!("{context}：{e}"))
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 只輸出訊息，不含類別 —— 與原本的 String 完全一致。
        f.write_str(&self.message)
    }
}

impl std::error::Error for CmdError {}

/// `String` → `CmdError`：保留文字，類別預設為 `Internal`。
///
/// 給漸進轉換用：某處改了、但呼叫它的上游還沒改時，用 `?` 就能自動接上，
/// 訊息不變。
impl From<String> for CmdError {
    fn from(s: String) -> Self {
        Self::internal(s)
    }
}

impl From<&str> for CmdError {
    fn from(s: &str) -> Self {
        Self::internal(s)
    }
}

/// `CmdError` → `String`：讓還沒轉換的命令簽章（`Result<_, String>`）可以
/// 直接 `?` 接住具型別錯誤，不必逐處 `.map_err`。
///
/// 這條轉換是漸進路線能成立的關鍵：**下游可以用新型別，上游照舊收 String。**
impl From<CmdError> for String {
    fn from(e: CmdError) -> Self {
        e.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最重要的一條：`Display` 必須與原本的 `String` 逐字相同，
    /// 否則改用 `CmdError` 會悄悄改掉 UI 顯示的文字。
    #[test]
    fn display_is_byte_identical_to_the_original_string() {
        let msg = "該來源不存在或已停用（可能已被刪除），請重新選擇來源接管";
        assert_eq!(CmdError::not_found(msg).to_string(), msg);
        assert_eq!(CmdError::internal(msg).to_string(), msg);
        assert_eq!(String::from(CmdError::conflict(msg)), msg);
    }

    /// 類別不該出現在顯示文字裡（它是給程式判斷用的）。
    #[test]
    fn display_does_not_leak_the_kind_code() {
        let e = CmdError::auth("無效的 Key");
        let shown = e.to_string();
        assert_eq!(shown, "無效的 Key");
        for k in [
            ErrKind::Invalid,
            ErrKind::NotFound,
            ErrKind::Conflict,
            ErrKind::Auth,
            ErrKind::Internal,
        ] {
            assert!(
                !shown.contains(k.code()),
                "顯示文字不該含類別代碼 {}：{shown}",
                k.code()
            );
        }
    }

    #[test]
    fn kind_is_preserved() {
        assert_eq!(CmdError::invalid("x").kind(), ErrKind::Invalid);
        assert_eq!(CmdError::not_found("x").kind(), ErrKind::NotFound);
        assert_eq!(CmdError::conflict("x").kind(), ErrKind::Conflict);
        assert_eq!(CmdError::auth("x").kind(), ErrKind::Auth);
        assert_eq!(CmdError::internal("x").kind(), ErrKind::Internal);
    }

    /// 漸進轉換的兩個方向都要能 `?` 接上。
    #[test]
    fn conversions_keep_the_message() {
        // String → CmdError（上游還是 String 的錯誤）
        let e: CmdError = "資料庫鎖定".to_string().into();
        assert_eq!(e.to_string(), "資料庫鎖定");
        assert_eq!(e.kind(), ErrKind::Internal, "來源不明的錯誤歸為 Internal");

        // CmdError → String（下游還是 Result<_, String> 的命令簽章）
        let s: String = CmdError::invalid("起始時間必須早於結束時間").into();
        assert_eq!(s, "起始時間必須早於結束時間");
    }

    #[test]
    fn wrap_includes_context_and_cause() {
        let e = CmdError::wrap("讀取設定檔失敗", std::io::Error::other("permission denied"));
        assert_eq!(e.kind(), ErrKind::Internal);
        assert!(e.message().starts_with("讀取設定檔失敗："), "{}", e.message());
        assert!(e.message().contains("permission denied"), "{}", e.message());
    }

    /// `?` 運算子要能在「回傳 CmdError」的函式裡接住 String 錯誤。
    #[test]
    fn question_mark_bridges_string_errors() {
        fn inner(fail: bool) -> Result<u8, String> {
            if fail {
                Err("來源已停用".into())
            } else {
                Ok(1)
            }
        }
        fn outer(fail: bool) -> Result<u8, CmdError> {
            let v = inner(fail)?; // String → CmdError
            Ok(v)
        }
        assert_eq!(outer(false).unwrap(), 1);
        let e = outer(true).unwrap_err();
        assert_eq!(e.to_string(), "來源已停用");
    }

    /// 反向：回傳 String 的函式接住 CmdError。
    #[test]
    fn question_mark_bridges_typed_errors() {
        fn typed(fail: bool) -> Result<u8, CmdError> {
            if fail {
                Err(CmdError::auth("無效的 Key"))
            } else {
                Ok(2)
            }
        }
        fn legacy(fail: bool) -> Result<u8, String> {
            let v = typed(fail)?; // CmdError → String
            Ok(v)
        }
        assert_eq!(legacy(false).unwrap(), 2);
        assert_eq!(legacy(true).unwrap_err(), "無效的 Key");
    }
}
