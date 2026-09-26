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

export function fmtRemain(iso: string | null): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  let s = Math.floor((t - Date.now()) / 1000);
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
export function QuotaRing(props: { w: QuotaWindow }) {
  const { w } = props;
  const pct = Math.max(0, Math.min(100, w.percent));
  const r = 15.5;
  const c = 2 * Math.PI * r;
  const col = "var(--mac-accent)";
  return (
    <div
      className="flex shrink-0 flex-col items-center gap-1.5"
      style={{ width: RING_BOX }}
    >
      <div className="relative" style={{ width: RING_DIAL, height: RING_DIAL }}>
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
        <div className="text-[11px] text-white/55">{w.label}</div>
        <div className="text-[10px] text-white/30">
          {w.resets_at ? `重置 ${fmtRemain(w.resets_at)}` : "—"}
        </div>
      </div>
    </div>
  );
}

export function QuotaCard(props: { q: SubscriptionQuota }) {
  const { q } = props;
  const ok = q.status === "ok" && q.windows.length > 0;
  // 名稱去掉括號說明（如「OpenCode Go（opencode-go）」→「OpenCode Go」）
  const title = q.title.replace(/[（(][^（）()]*[）)]/g, "").trim() || q.title;
  return (
    <div className="glass-float float-hover anim-rise w-fit max-w-full min-w-0 p-4">
      <div className="mb-2 flex min-w-0 items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-white/85">
          {title}
        </span>
      </div>
      {ok ? (
        <div className="flex flex-wrap items-start justify-around gap-x-2 gap-y-3">
          {q.windows.map((w) => (
            <QuotaRing key={w.label} w={w} />
          ))}
        </div>
      ) : (
        <p className="text-xs break-words text-white/35">
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
    staleTime: 5 * 60_000,
    refetchInterval: 5 * 60_000,
  });
  // 只顯示實際有訂閱/可查到額度的方案；未訂閱或無憑證的不佔位
  const list = (q.data || []).filter(
    (x) => x.status === "ok" && x.windows.length > 0,
  );
  if (!q.isPending && list.length === 0) return null;
  return (
    <div className="glass min-w-0 p-4 md:p-5">
      <div className="mb-3 flex items-center gap-2">
        <span className="text-sm font-semibold tracking-tight text-white/80">
          訂閱額度
        </span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-white/25">
          {q.isFetching
            ? "查詢中…"
            : q.dataUpdatedAt
              ? `更新於 ${new Date(q.dataUpdatedAt).toLocaleTimeString()}`
              : ""}
        </span>
        <button
          className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
          disabled={q.isFetching}
          onClick={() => void qc.invalidateQueries({ queryKey: ["quota_all"] })}
        >
          <Icon name="refresh" size={11} />
          重新查詢
        </button>
      </div>
      {q.isPending ? (
        <p className="text-sm text-white/30">查詢中…</p>
      ) : (
        <div className="flex min-w-0 flex-wrap items-start gap-3">
          {list.map((x) => (
            <QuotaCard key={x.key} q={x} />
          ))}
        </div>
      )}
    </div>
  );
}
