//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import {
  api,
  type QuotaWindow,
  type SubscriptionQuota,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  useEffect,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";

/** 訂閱額度自動重查間隔（對齊 cc-switch `REFETCH_INTERVAL` ＝ 5 分）。 */
const QUOTA_REFRESH_MS = 5 * 60_000;
/** 本地心跳：倒數與「更新於」每 30 秒走一格，不必等下一次網路重查。 */
const TICK_MS = 30_000;

/** 每 TICK_MS 心跳一次，逼倒數字與「更新於」即時走動。
 *  計時文字是在 render 時用 `Date.now()` 算出來的：沒有心跳就不會重繪，
 * 畫面上的「重置 3時27分」會整段 freeze，看起來就像「沒有即時更新」。 */
export function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

/** 「更新於」＝ 查詢時刻 ＋ 已經過多久：陳舊度一眼可見，不用自己對時鐘。 */
export function fmtQueriedAt(at: number, now: number): string {
  const clock = new Date(at).toLocaleTimeString();
  const s = Math.max(0, Math.floor((now - at) / 1000));
  if (s < 60) return clock;
  if (s < 3600) return `${clock}（${Math.floor(s / 60)} 分鐘前）`;
  if (s < 86400) return `${clock}（${Math.floor(s / 3600)} 小時前）`;
  return `${clock}（${Math.floor(s / 86400)} 天前）`;
}

export function fmtRemain(iso: string | null, now: number = Date.now()): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  let s = Math.floor((t - now) / 1000);
  if (s <= 0) return "已重置";
  const d = Math.floor(s / 86400);
  s %= 86400;
  const h = Math.floor(s / 3600);
  s %= 3600;
  const m = Math.floor(s / 60);
  if (d > 0) return `${d}天${h}時`;
  if (h > 0) return `${h}時${m}分`;
  if (m > 0) return `${m}分`;
  return `${s}秒`;
}

/** Apple 健康風圓環：進度弧 + 中心百分比，下方窗口名與重置倒計時。顏色跟隨系統強調色。 */
/** 訂閱圓環統一尺寸：所有方案、所有窗口共用同一組數字，視覺保證一致。 */
export const RING_BOX = 76; // 外層寬 px
export const RING_DIAL = 68; // 錶盤直徑 px
export const RING_TEXT = 13; // 中心百分比字號 px
export function QuotaRing(props: { w: QuotaWindow; now: number }) {
  const { w, now } = props;
  const pct = Math.max(0, Math.min(100, w.percent));
  const r = 15.5;
  const c = 2 * Math.PI * r;
  const col = "var(--mac-accent)";
  return (
    <div
      className="flex shrink-0 flex-col items-center gap-1.5"
      style={{ width: RING_BOX }}
    >
      <div
        className="relative"
        style={{ width: RING_DIAL, height: RING_DIAL }}
        title="已使用百分比（上游各窗口的 percent）"
      >
        <svg viewBox="0 0 36 36" className="h-full w-full -rotate-90">
          <circle
            cx="18"
            cy="18"
            r={r}
            fill="none"
            stroke="rgba(255,255,255,0.08)"
            strokeWidth="3.4"
          />
          <circle
            cx="18"
            cy="18"
            r={r}
            fill="none"
            stroke={col}
            strokeWidth="3.4"
            strokeLinecap="round"
            strokeDasharray={`${(pct / 100) * c} ${c}`}
            style={{
              transition:
                "stroke-dasharray 0.6s var(--mac-ease), stroke 0.3s var(--mac-ease)",
              filter:
                "drop-shadow(0 0 4px color-mix(in srgb, var(--mac-accent) 40%, transparent))",
            }}
          />
        </svg>
        <div
          className="absolute inset-0 flex items-center justify-center font-semibold tabular-nums"
          style={{ color: col, fontSize: RING_TEXT }}
        >
          {w.percent < 10 ? w.percent.toFixed(1) : Math.round(w.percent)}%
        </div>
      </div>
      <div className="text-center leading-tight">
        <div className="text-[11px] text-fg/55">{w.label}</div>
        <div className="text-[10px] text-fg/30">
          {w.resets_at ? `重置 ${fmtRemain(w.resets_at, now)}` : "—"}
        </div>
      </div>
    </div>
  );
}

export function QuotaCard(props: { q: SubscriptionQuota; now: number }) {
  const { q, now } = props;
  const ok = q.status === "ok" && q.windows.length > 0;
  // 名稱去掉括號說明（如「OpenCode Go（opencode-go）」→「OpenCode Go」）
  const title = q.title.replace(/[（(][^（）()]*[）)]/g, "").trim() || q.title;
  return (
    <div className="glass-float float-hover anim-rise w-fit max-w-full min-w-0 p-4">
      <div className="mb-2 flex min-w-0 items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-fg/85">
          {title}
        </span>
      </div>
      {ok ? (
        <div className="flex flex-wrap items-start justify-around gap-x-2 gap-y-3">
          {q.windows.map((w) => (
            <QuotaRing key={w.label} w={w} now={now} />
          ))}
        </div>
      ) : (
        <p className="text-xs break-words text-fg/35">
          {q.message || "暫無額度數據"}
        </p>
      )}
    </div>
  );
}

export function QuotaSection() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ["quota_all"],
    queryFn: api.quotaQueryAll,
    staleTime: QUOTA_REFRESH_MS,
    refetchInterval: QUOTA_REFRESH_MS,
    // 視窗最小化／在背景時一樣要照跑：這是常駐（縮到匣）的工具，最長的時間
    // 視窗都是隱藏的，沒有這行的話 polling 會整段停擺，回到畫面只剩舊數字。
    // 對齊 cc-switch src/lib/query/subscription.ts 的 refetchIntervalInBackground。
    refetchIntervalInBackground: true,
    refetchOnWindowFocus: true,
    retry: 1,
  });
  const now = useNow(!q.isPending);
  // 心跳當 watchdog：**不管輪詢為什麼停掉**（計時器被系統凍結、焦點／visibility 事件
  // 沒送到、react-query 的 interval 被清掉），只要資料比兩個週期還舊就強制重查一次。
  // 這是「卡片卡在幾小時前的數字」的保底：最壞情況也只會舊約 10 分鐘。
  // 冷卻一個週期，避免上游一直失敗時每 30 秒打一次。
  const forcedAt = useRef(0);
  useEffect(() => {
    if (q.isPending || q.isFetching || !q.dataUpdatedAt) return;
    if (now - q.dataUpdatedAt < QUOTA_REFRESH_MS * 2) return;
    if (now - forcedAt.current < QUOTA_REFRESH_MS) return;
    forcedAt.current = now;
    void qc.invalidateQueries({ queryKey: ["quota_all"] });
  }, [now, q.dataUpdatedAt, q.isPending, q.isFetching, qc]);
  // 視窗重新拿到焦點（尤其是**從系統匣叫回來**）時立刻重查。
  // react-query 的焦點重取只掛在 `visibilitychange`，而 Tauri 的 hide()/show() 不保證
  // 會送那個事件（實測：縮到匣裡 `document.visibilityState` 仍是 visible）—— 靠這裡
  // 補上，使用者回到畫面第一眼看到的就是新數字，而不是上一次開著時的舊數字。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    const again = () => void qc.invalidateQueries({ queryKey: ["quota_all"] });
    window.addEventListener("focus", again);
    void listen("tauri://focus", again).then((f) => {
      if (cancelled) f();
      else unlisten = f;
    });
    return () => {
      cancelled = true;
      window.removeEventListener("focus", again);
      unlisten?.();
    };
  }, [qc]);
  // 只顯示實際有訂閱/可查到額度的方案；未訂閱或無憑證的不佔位
  const list = (q.data || []).filter(
    (x) => x.status === "ok" && x.windows.length > 0,
  );
  // 過期就別再若無其事地顯示：琥珀色 ＋「可能已過期」，讓「看起來像現在」的假象消失
  const age = q.dataUpdatedAt ? now - q.dataUpdatedAt : 0;
  const expired = age > QUOTA_REFRESH_MS * 1.5;
  if (!q.isPending && list.length === 0) return null;
  return (
    <div className="glass min-w-0 p-4 md:p-5">
      <div className="mb-3 flex items-center gap-2">
        <span className="text-sm font-semibold tracking-tight text-fg/80">
          訂閱額度
        </span>
        <span
          className="shrink-0 text-[11px] text-fg/25"
          title="圓環裡的數字是「已使用」百分比。opencode.ai 等後台頁顯示的是「剩餘」百分比，兩者相加剛好 100%（例如後台 97% left ＝ 這裡 3%）。"
        >
          圓環＝已用 %
        </span>
        <span
          className={`min-w-0 flex-1 truncate text-[11px] ${
            expired ? "text-amber-300/80" : "text-fg/25"
          }`}
          title={
            expired
              ? "這份額度資料已經超過一個輪詢週期沒更新，數字可能不是最新的"
              : undefined
          }
        >
          {q.isFetching
            ? "查詢中…"
            : q.dataUpdatedAt
              ? `更新於 ${fmtQueriedAt(q.dataUpdatedAt, now)}${expired ? " · 可能已過期" : ""}`
              : ""}
        </span>
        <button
          className="flex shrink-0 items-center gap-1 rounded-full border border-fg/10 px-2.5 py-0.5 text-[11px] text-fg/50 transition-colors hover:border-fg/20 hover:text-fg disabled:opacity-40"
          disabled={q.isFetching}
          onClick={() => void qc.invalidateQueries({ queryKey: ["quota_all"] })}
        >
          <Icon name="refresh" size={11} />
          重新查詢
        </button>
      </div>
      {q.isPending ? (
        <p className="text-sm text-fg/30">查詢中…</p>
      ) : (
        <div className="flex min-w-0 flex-wrap items-start gap-3">
          {list.map((x) => (
            <QuotaCard key={x.key} q={x} now={now} />
          ))}
        </div>
      )}
    </div>
  );
}
