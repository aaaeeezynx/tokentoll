//! 來源健康狀態（斷路器）—— 抄 cc-switch 的 circuit breaker。
//!
//! cc-switch 的 failover queue 旁邊有一個斷路器：某個來源連續失敗數次就先「跳開」
//! 一段時間，不要每個請求都去撞同一面牆。這裡照抄那個精神，但**改成「排到最後」
//! 而不是「跳過」**：
//!
//! - 這是本機工具唯一的出口，全部來源都被跳過時使用者只會看到失敗；
//!   排到最後則在「還有健康的來源」時省下必然失敗的等待，在「沒有別的來源」時
//!   行為與從前**完全一樣**（候選只有一個，排到哪都還是它）。
//! - 冷卻期滿自動放行一次（half-open）：成功就恢復，失敗就立刻再跳開。
//!
//! ## 什麼算「不健康」
//!
//! 只有**連不上**（連線失敗 → 502）與**上游 5xx** 算。4xx 一律不算，
//! 因為那代表來源活著、只是這個請求有問題；把 400 也算進去會讓協議換手
//! 自己把自己絆倒（`grok-4.7` 第一次必定先吃一個 400 才知道要換協議）。
//!
//! ## 為什麼不落庫
//!
//! 與「學到的協議」刻意相反：協議是上游的**客觀事實**，重啟不該忘記；
//! 斷路器是**短命的健康判斷**，重啟代表重新開始，留著舊的判定只會讓使用者
//! 覺得「明明已經好了卻還被跳過」。

use super::*;
use std::sync::OnceLock;
use std::time::Duration;

/// 連續失敗幾次就跳開。
pub(super) const FAIL_THRESHOLD: u32 = 3;
/// 跳開後冷卻多久（秒）。
pub(super) const COOLDOWN_SECS: u64 = 60;

/// 單一來源的健康狀態。
#[derive(Debug, Clone, Default)]
struct Health {
    consecutive_failures: u32,
    /// `Some(t)`：在 `t` 之前不優先嘗試（時間過了就自動放行）。
    open_until: Option<Instant>,
    last_error: Option<String>,
    last_failure_at: Option<Instant>,
    last_success_at: Option<Instant>,
}

/// 對外揭露的一列健康狀態（給診斷頁；不含 `Instant`，只有相對秒數）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HealthRow {
    pub provider_id: i64,
    pub open: bool,
    pub consecutive_failures: u32,
    /// 還要冷卻幾秒（0 = 沒在冷卻）。
    pub cooldown_secs: u64,
    pub last_error: String,
    /// 距離上次失敗幾秒（沒失敗過 = `None`）。
    pub secs_since_failure: Option<u64>,
    /// 距離上次成功幾秒（沒成功過 = `None`）。
    pub secs_since_success: Option<u64>,
}

/// 全部來源的健康狀態。`Clone` 共用同一份資料（`Arc`）。
#[derive(Clone)]
pub struct HealthRegistry {
    inner: Arc<Mutex<HashMap<i64, Health>>>,
    threshold: u32,
    cooldown: Duration,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::with_policy(FAIL_THRESHOLD, Duration::from_secs(COOLDOWN_SECS))
    }
}

/// 網關行程內共用的那一份：以**資料庫路徑**為範圍（正式路徑只有一個資料庫，
/// 也就是只有一個網關；測試則各自 `serve()` 自己的庫，因此互不干擾）。
///
/// 用 `db_path` 當範圍是刻意的：`provider_id` 只在單一資料庫內唯一，兩個不同的
/// 資料庫可以有同一個 id。這件事在「學到的協議」上是靠資料庫表本身解決，
/// 這裡則是靠這層雜湊。
static GLOBAL: OnceLock<Mutex<HashMap<PathBuf, HealthRegistry>>> = OnceLock::new();

/// 取「這個資料庫」的健康狀態表（第一次呼叫時建立）。
pub fn for_db(db_path: &std::path::Path) -> HealthRegistry {
    let m = GLOBAL.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = m.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(db_path.to_path_buf()).or_default().clone()
}

impl HealthRegistry {
    /// 自訂門檻與冷卻時間（測試用；正式路徑走 `Default`）。
    pub(super) fn with_policy(threshold: u32, cooldown: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            threshold: threshold.max(1),
            cooldown,
        }
    }

    pub(crate) fn record_failure(&self, provider_id: i64, err: &str) {
        let now = Instant::now();
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let h = map.entry(provider_id).or_default();
        h.consecutive_failures = h.consecutive_failures.saturating_add(1);
        h.last_error = Some(shorten(err));
        h.last_failure_at = Some(now);
        if h.consecutive_failures >= self.threshold {
            h.open_until = Some(now + self.cooldown);
        }
    }

    pub(crate) fn record_success(&self, provider_id: i64) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let h = map.entry(provider_id).or_default();
        h.consecutive_failures = 0;
        h.open_until = None;
        h.last_error = None;
        h.last_success_at = Some(Instant::now());
    }

    /// 這個來源現在是不是「跳開中」（冷卻期內不優先嘗試）。
    pub(super) fn is_open(&self, provider_id: i64) -> bool {
        let now = Instant::now();
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&provider_id).is_some_and(|h| is_open_at(h, now))
    }

    /// 目前所有「有記錄」的來源（完全沒被請求過的來源不會有列）。
    pub(crate) fn snapshot(&self) -> Vec<HealthRow> {
        let now = Instant::now();
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<HealthRow> = map
            .iter()
            .map(|(pid, h)| HealthRow {
                provider_id: *pid,
                open: is_open_at(h, now),
                consecutive_failures: h.consecutive_failures,
                cooldown_secs: cooldown_left(h, now),
                last_error: h.last_error.clone().unwrap_or_default(),
                secs_since_failure: h
                    .last_failure_at
                    .map(|t| now.saturating_duration_since(t).as_secs()),
                secs_since_success: h
                    .last_success_at
                    .map(|t| now.saturating_duration_since(t).as_secs()),
            })
            .collect();
        // 跳開中的排前面，其次失敗次數多的 —— 診斷頁第一眼就該看到有問題的。
        out.sort_by(|a, b| {
            b.open
                .cmp(&a.open)
                .then(b.consecutive_failures.cmp(&a.consecutive_failures))
                .then(a.provider_id.cmp(&b.provider_id))
        });
        out
    }
}

fn is_open_at(h: &Health, now: Instant) -> bool {
    h.open_until.is_some_and(|t| t > now)
}

fn cooldown_left(h: &Health, now: Instant) -> u64 {
    match h.open_until {
        Some(t) if t > now => t.saturating_duration_since(now).as_secs() + 1,
        _ => 0,
    }
}

/// 錯誤字串截斷（會進診斷頁，不該是整包上游回應）。
fn shorten(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 120 {
        return s.to_string();
    }
    let cut: String = s.chars().take(120).collect();
    format!("{cut}…")
}

/// 把「跳開中」的來源排到最後（穩定排序：同組內維持原本的優先序）。
///
/// 候選**一個都不會被丟掉** —— 全部都在跳開時，順序就等於原順序。
pub(super) fn order_by_health(attempts: Vec<Attempt>, reg: &HealthRegistry) -> Vec<Attempt> {
    if attempts.len() < 2 {
        return attempts;
    }
    let (mut ok, mut cooling): (Vec<Attempt>, Vec<Attempt>) = (Vec::new(), Vec::new());
    for a in attempts {
        if reg.is_open(a.provider_id) {
            cooling.push(a);
        } else {
            ok.push(a);
        }
    }
    if cooling.is_empty() {
        return ok;
    }
    ok.extend(cooling);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pid: i64, fmt: &str) -> Attempt {
        Attempt::new(pid, fmt)
    }

    fn ids(v: &[Attempt]) -> Vec<i64> {
        v.iter().map(|a| a.provider_id).collect()
    }

    fn registry() -> HealthRegistry {
        HealthRegistry::with_policy(3, Duration::from_secs(60))
    }

    /// 連續失敗達門檻才跳開；未達門檻前照常。
    #[test]
    fn circuit_opens_only_after_threshold() {
        let r = registry();
        r.record_failure(7, "boom");
        r.record_failure(7, "boom");
        assert!(!r.is_open(7), "兩次還不該跳開");
        assert_eq!(r.snapshot()[0].consecutive_failures, 2);
        r.record_failure(7, "boom");
        assert!(r.is_open(7), "第三次要跳開");
        let row = &r.snapshot()[0];
        assert!(row.open && row.cooldown_secs > 0 && row.cooldown_secs <= 60);
    }

    /// 成功即恢復（並清掉錯誤訊息與失敗計數）。
    #[test]
    fn success_resets_circuit() {
        let r = registry();
        for _ in 0..3 {
            r.record_failure(7, "boom");
        }
        assert!(r.is_open(7));
        r.record_success(7);
        assert!(!r.is_open(7));
        let row = &r.snapshot()[0];
        assert_eq!(row.consecutive_failures, 0);
        assert!(row.last_error.is_empty());
        assert!(row.secs_since_success.is_some());
    }

    /// 冷卻期滿自動放行（half-open）；此時再失敗一次就立刻重新跳開。
    #[test]
    fn cooldown_expires_and_one_failure_reopens() {
        let r = HealthRegistry::with_policy(2, Duration::from_millis(60));
        r.record_failure(7, "boom");
        r.record_failure(7, "boom");
        assert!(r.is_open(7));
        std::thread::sleep(Duration::from_millis(90));
        assert!(!r.is_open(7), "冷卻期滿就該再給它機會");
        r.record_failure(7, "still down");
        assert!(r.is_open(7), "探測失敗要立刻再跳開，不必再等三次");
    }

    /// 每個來源各自記帳。
    #[test]
    fn providers_are_tracked_separately() {
        let r = registry();
        for _ in 0..3 {
            r.record_failure(7, "boom");
        }
        r.record_failure(9, "boom");
        assert!(r.is_open(7));
        assert!(!r.is_open(9));
        // 跳開中的排前面
        assert_eq!(ids_rows(&r.snapshot()), vec![7, 9]);
    }

    fn ids_rows(v: &[HealthRow]) -> Vec<i64> {
        v.iter().map(|r| r.provider_id).collect()
    }

    /// 跳開的來源被排到最後，但**沒有被丟掉**。
    #[test]
    fn open_provider_goes_last_but_is_kept() {
        let r = registry();
        for _ in 0..3 {
            r.record_failure(2, "boom");
        }
        let ordered = order_by_health(
            vec![
                at(2, "openai-chat"),
                at(2, "openai-responses"),
                at(5, "openai-chat"),
            ],
            &r,
        );
        assert_eq!(ids(&ordered), vec![5, 2, 2]);
        assert_eq!(ordered.len(), 3, "候選不能被丟掉");
    }

    /// 全部來源都跳開時，順序必須與原本完全相同（否則等於全部失敗）。
    #[test]
    fn all_open_keeps_original_order() {
        let r = registry();
        for pid in [1, 2] {
            for _ in 0..3 {
                r.record_failure(pid, "boom");
            }
        }
        let ordered = order_by_health(vec![at(1, "openai-chat"), at(2, "mixed")], &r);
        assert_eq!(ids(&ordered), vec![1, 2]);
    }

    /// 錯誤訊息截斷（診斷頁不該塞整包上游回應）。
    #[test]
    fn error_text_is_truncated() {
        let r = registry();
        r.record_failure(7, &"x".repeat(500));
        let row = &r.snapshot()[0];
        assert_eq!(row.last_error.chars().count(), 121, "120 字 + 省略號");
        assert!(row.last_error.ends_with('…'));
    }
}
