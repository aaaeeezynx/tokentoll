//! 來源／協議的自動換手。
//!
//! 為什麼需要這一層：**同一個來源的模型可能逐模型只在一種端點上架**。2026-09-30
//! 對 opencode-go 直接量測：`grok-4.7`／`gpt-6-luna`／`muse-spark-1.3-contributor`
//! 只在 `/responses` 有回應，`mimo-v2.6-pro`／`flash` 只在 `/chat/completions`，
//! `deepseek-*` 兩邊都有。所以「來源協議選對了」不等於「這個模型打得通」：
//! 上游會回 `400 ModelProtocolUnsupported`。
//!
//! 這裡把「候選（來源, 協議）」排成一條佇列（抄 cc-switch 的 failover queue 精神），
//! 上游回協議不支援、連線失敗或 5xx 就換下一個；**並且補上 cc-switch 沒做的那一格**：
//! 同一個來源也換協議重試（Responses 入站時 chat ↔ responses 都生得出請求體）。
//! 成功後把「這個來源的這個模型該用哪個協議」記在記憶體，下次第一個就試它。
//!
//! 已知限制（照實寫在文件裡）：Anthropic 入站的請求**只生得出 chat 請求體**
//! （`Anthropic → Responses` 沒有翻譯器，矩陣直接回 `E_ANTHROPIC_UNSUPPORTED`），
//! 所以 responses-only 的模型在 Claude Code 永遠換不過去。

use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

/// 一次上游嘗試：用哪個來源、哪個協議。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Attempt {
    pub(super) provider_id: i64,
    pub(super) api_format: String,
}

impl Attempt {
    pub(super) fn new(provider_id: i64, api_format: &str) -> Self {
        Self {
            provider_id,
            api_format: api_format.to_string(),
        }
    }
}

/// 學到的協議：`(db_path, provider_id, model, 宣告協議) -> 實際會通的協議`。
///
/// 兩個維度都是必要的：
/// - **`db_path`**：`provider_id` 只在單一資料庫內唯一（測試各自開暫存庫，使用者
///   重置後也會拿到同樣的 id）。少了它，A 資料庫學到的會被 B 資料庫沿用。
/// - **宣告協議**：使用者把來源協議從 `openai-chat` 改成 `openai-responses` 是
///   **明確的設定變更**，不可以被舊的學習結果蓋掉（那會讓「改了設定卻沒生效」）。
///   所以學到的值只在「宣告協議沒變」時才拿來排第一。
///
/// 只放記憶體（v1）：網關重啟後每個模型要多付一次失敗探測，但不會有跨版本的
/// 殘留錯資料。之後若要持久化，再加一張表並在啟動時載入即可。
/// 學習快取的鍵與表（見下方 `LEARNED` 的說明）。
type LearnKey = (String, i64, String, String);
type Learned = HashMap<LearnKey, String>;

static LEARNED: Mutex<Option<Learned>> = Mutex::new(None);

fn with_learned<T>(f: impl FnOnce(&mut Learned) -> T) -> T {
    let mut guard = LEARNED.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    f(map)
}

/// 記住「這個資料庫的這個來源、這個模型，在宣告協議 X 之下用 Y 會通」。
pub(super) fn learn_format(
    db_path: &str,
    provider_id: i64,
    model: &str,
    declared_fmt: &str,
    used_fmt: &str,
) {
    if model.trim().is_empty() || used_fmt.trim().is_empty() {
        return;
    }
    with_learned(|m| {
        m.insert(
            (
                db_path.to_string(),
                provider_id,
                model.to_lowercase(),
                declared_fmt.to_lowercase(),
            ),
            used_fmt.to_string(),
        );
    });
}

/// 上次學到的協議（沒有就 `None`）。**只在宣告協議相同時才回傳。**
pub(super) fn learned_format(
    db_path: &str,
    provider_id: i64,
    model: &str,
    declared_fmt: &str,
) -> Option<String> {
    if model.trim().is_empty() {
        return None;
    }
    with_learned(|m| {
        m.get(&(
            db_path.to_string(),
            provider_id,
            model.to_lowercase(),
            declared_fmt.to_lowercase(),
        ))
        .cloned()
    })
}

/// 測試用：清空學習快取（避免測試之間互相影響）。
#[cfg(test)]
pub(super) fn forget_all() {
    with_learned(|m| m.clear());
}

/// 這個協議「另一邊」有哪些可換的具體協議。
///
/// `mixed` 不是一種真的線上協議（它的意思是「兩種端點都直通」），所以它的另一邊
/// 同時包含 chat 與 responses：遇到 chat-only 的模型時要換成 `openai-chat`
/// （翻譯那條），遇到 responses-only 時直通本來就會過。
fn other_sides(fmt: &str) -> &'static [&'static str] {
    match fmt {
        "openai-chat" => &["openai-responses"],
        "openai-responses" => &["openai-chat"],
        "mixed" => &["openai-chat", "openai-responses"],
        _ => &[],
    }
}

/// 換協議是否有意義：矩陣必須支援，**而且**翻譯種類真的不同。
///
/// - Responses 入站：chat→responses＝直通 vs 翻譯，明顯不同 ✅
/// - Anthropic 入站：chat→responses 矩陣直接 Err（沒有 Anthropic→Responses 翻譯器）❌
/// - OpenAI chat 入站：chat→responses 也是 Err（B6 的明確報錯）❌
fn flip_is_useful(in_fmt: InFmt, from: &str, to: &str) -> bool {
    match (
        resolve_trans_kind(in_fmt, TargetFmt::from_db(from)),
        resolve_trans_kind(in_fmt, TargetFmt::from_db(to)),
    ) {
        (Ok(a), Ok(b)) => a != b,
        _ => false,
    }
}

/// 排候選佇列（去重、保序）。
///
/// 順序：① 上次學到的協議 ② 綁定來源的宣告協議 ③ 綁定來源的另一種協議
/// ④ 其他也登記了這個模型的來源（各自：宣告協議 → 另一種協議，依 priority/id 排序）。
pub(super) fn plan_attempts(
    bound_pid: i64,
    bound_fmt: &str,
    in_fmt: InFmt,
    learned: Option<&str>,
    others: &[ReroutedProvider],
) -> Vec<Attempt> {
    let mut out: Vec<Attempt> = Vec::new();
    let add = |out: &mut Vec<Attempt>, pid: i64, fmt: &str| {
        let fmt = fmt.trim();
        if fmt.is_empty() {
            return;
        }
        if out
            .iter()
            .any(|a| a.provider_id == pid && a.api_format.eq_ignore_ascii_case(fmt))
        {
            return;
        }
        out.push(Attempt::new(pid, fmt));
    };
    if let Some(l) = learned {
        add(&mut out, bound_pid, l);
    }
    add(&mut out, bound_pid, bound_fmt);
    for f in other_sides(bound_fmt) {
        if flip_is_useful(in_fmt, bound_fmt, f) {
            add(&mut out, bound_pid, f);
        }
    }
    for p in others {
        add(&mut out, p.id, &p.format);
        for f in other_sides(&p.format) {
            if flip_is_useful(in_fmt, &p.format, f) {
                add(&mut out, p.id, f);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov(id: i64, fmt: &str) -> ReroutedProvider {
        ReroutedProvider {
            id,
            base_url: format!("http://127.0.0.1:{id}"),
            api_key: "k".into(),
            scheme: "bearer".into(),
            format: fmt.into(),
        }
    }

    fn ids(v: &[Attempt]) -> Vec<(i64, String)> {
        v.iter()
            .map(|a| (a.provider_id, a.api_format.clone()))
            .collect()
    }

    /// Codex（Responses 入站）：來源宣告 chat 但模型只在 responses 上架 → 補一條直通候選。
    #[test]
    fn responses_inbound_gets_responses_fallback_for_chat_provider() {
        let a = plan_attempts(7, "openai-chat", InFmt::Responses, None, &[]);
        assert_eq!(
            ids(&a),
            vec![(7, "openai-chat".into()), (7, "openai-responses".into())]
        );
    }

    /// `mixed` 是「兩種端點都直通」，但 chat-only 的模型要換成 chat（翻譯那條）。
    #[test]
    fn mixed_provider_gets_chat_fallback_for_chat_only_models() {
        let a = plan_attempts(7, "mixed", InFmt::Responses, None, &[]);
        assert_eq!(
            ids(&a),
            vec![(7, "mixed".into()), (7, "openai-chat".into())]
        );
    }

    /// Claude Code（Anthropic 入站）：翻 responses 沒有翻譯器 → 不要排無效候選。
    #[test]
    fn anthropic_inbound_has_no_responses_fallback() {
        let a = plan_attempts(7, "openai-chat", InFmt::Anthropic, None, &[]);
        assert_eq!(ids(&a), vec![(7, "openai-chat".into())]);
    }

    /// 學到的協議排第一；與宣告相同時去重。
    #[test]
    fn learned_format_goes_first_and_dedupes() {
        let a = plan_attempts(7, "openai-chat", InFmt::Responses, Some("openai-responses"), &[]);
        assert_eq!(ids(&a)[0], (7, "openai-responses".into()));
        let b = plan_attempts(7, "openai-chat", InFmt::Responses, Some("openai-chat"), &[]);
        assert_eq!(
            ids(&b),
            vec![(7, "openai-chat".into()), (7, "openai-responses".into())]
        );
    }

    /// 其他來源排在綁定來源之後（來源換手是備援，不是首選）。
    #[test]
    fn other_providers_come_after_bound_one() {
        let others = vec![prov(3, "openai-chat"), prov(5, "openai-responses")];
        let a = plan_attempts(7, "openai-chat", InFmt::OpenAi, None, &others);
        // chat 入站翻 responses 是矩陣明定的錯誤（B6），所以只排各來源的宣告協議。
        assert_eq!(
            ids(&a),
            vec![
                (7, "openai-chat".into()),
                (3, "openai-chat".into()),
                (5, "openai-responses".into())
            ]
        );
    }

    #[test]
    fn learn_format_is_scoped_by_db_and_declared_format() {
        forget_all();
        assert!(learned_format("t.db", 1, "m", "openai-chat").is_none());
        learn_format("t.db", 1, "M", "openai-chat", "openai-responses");
        // 大小寫不敏感
        assert_eq!(
            learned_format("t.db", 1, "m", "openai-chat").as_deref(),
            Some("openai-responses")
        );
        // 別的來源、別的資料庫都不可以看到
        assert!(learned_format("t.db", 2, "m", "openai-chat").is_none());
        assert!(learned_format("other.db", 1, "m", "openai-chat").is_none());
        // ★ 宣告協議改了就是明確的設定變更 → 舊的學習結果不可以蓋掉它
        assert!(learned_format("t.db", 1, "m", "openai-responses").is_none());
        // 空字串不進快取（沒有模型名時無從學習）
        learn_format("t.db", 1, "", "openai-chat", "openai-chat");
        assert!(learned_format("t.db", 1, "", "openai-chat").is_none());
        forget_all();
        assert!(learned_format("t.db", 1, "m", "openai-chat").is_none());
    }
}
