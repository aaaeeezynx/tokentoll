//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import { type TrendAppRow } from "../../lib/api";
import { appLabel } from "../logos";
import {
  APP_ORDER,
  appColor,
  fmtBucket,
  fmtCost,
  fmtInt,
} from "./usageTypes";
import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";

export interface StackSeg {
  app: string;
  tokens: number;
}
export interface StackBucket {
  ts: number;
  total: number;
  requests: number;
  cost_usd: number;
  segs: StackSeg[];
}

export function fillAppBuckets(
  rows: TrendAppRow[],
  step: number,
  fillToday: boolean,
): { buckets: StackBucket[]; apps: string[]; totals: Record<string, number> } {
  if (rows.length === 0 && !fillToday)
    return { buckets: [], apps: [], totals: {} };
  // 今日固定補滿 00:00–23:00 共 24 桶（與後端整點桶對齊，UTC+8 整點一致）
  let minTs: number;
  let maxTs: number;
  if (fillToday) {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    minTs = d.getTime();
    maxTs = minTs + 23 * 3600 * 1000;
  } else {
    if (rows.length === 0) return { buckets: [], apps: [], totals: {} };
    minTs = Math.min(...rows.map((r) => r.ts));
    maxTs = Math.max(...rows.map((r) => r.ts));
  }
  const apps = Array.from(new Set(rows.map((r) => r.app))).sort((a, b) => {
    const oa = APP_ORDER.includes(a) ? APP_ORDER.indexOf(a) : 99;
    const ob = APP_ORDER.includes(b) ? APP_ORDER.indexOf(b) : 99;
    return oa - ob || (a < b ? -1 : 1);
  });
  const byKey = new Map(rows.map((r) => [`${r.ts}|${r.app}`, r]));
  const totals: Record<string, number> = {};
  for (const a of apps) totals[a] = 0;
  const buckets: StackBucket[] = [];
  for (let t = minTs; t <= maxTs; t += step) {
    const segs: StackSeg[] = [];
    let total = 0;
    let requests = 0;
    let cost = 0;
    for (const a of apps) {
      const r = byKey.get(`${t}|${a}`);
      const tokens = r?.tokens ?? 0;
      if (tokens > 0) {
        segs.push({ app: a, tokens });
        totals[a] += tokens;
      }
      total += tokens;
      requests += r?.requests ?? 0;
      cost += r?.cost_usd ?? 0;
    }
    buckets.push({ ts: t, total, requests, cost_usd: cost, segs });
  }
  return { buckets, apps, totals };
}

/** 圖表浮層提示：取代瀏覽器原生 title，定位於錨點元素上方（不足時翻到下方）。 */
export type TipRow = { dot?: string; label: string; value: string };

export function HoverTip(props: {
  rect: DOMRect | null;
  rootRef: RefObject<HTMLDivElement | null>;
  title?: string;
  rows?: TipRow[];
  children?: ReactNode;
}) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    setSize((s) => (s.w === w && s.h === h ? s : { w, h }));
  }, [props.rect, props.title, props.rows, props.children]);
  if (!props.rect) return null;
  const root = props.rootRef.current?.getBoundingClientRect();
  if (!root) return null;
  const w = size.w || 180;
  const h = size.h || 72;
  let left = props.rect.left - root.left + props.rect.width / 2 - w / 2;
  left = Math.max(4, Math.min(left, Math.max(4, root.width - w - 4)));
  let top = props.rect.top - root.top - h - 10;
  if (top < 4) top = props.rect.bottom - root.top + 10;
  return (
    <div
      ref={boxRef}
      className="tip-pop glass-float pointer-events-none absolute z-30 px-3 py-2"
      style={{
        left,
        top,
        minWidth: 170,
        maxWidth: 280,
        visibility: size.w ? "visible" : "hidden",
      }}
    >
      {props.title && (
        <div className="mb-1 text-[11px] font-semibold text-fg/85">
          {props.title}
        </div>
      )}
      {props.rows && props.rows.length > 0 && (
        <div className="space-y-0.5">
          {props.rows.map((r) => (
            <div
              key={r.label}
              className="flex items-center gap-2 text-[11px] leading-tight"
            >
              {r.dot ? (
                <span
                  className="h-2 w-2 shrink-0 rounded-sm"
                  style={{ background: r.dot, boxShadow: `0 0 5px 0 ${r.dot}` }}
                />
              ) : (
                <span className="w-2 shrink-0" />
              )}
              <span className="min-w-0 flex-1 truncate text-fg/40">
                {r.label}
              </span>
              <span className="shrink-0 font-medium tabular-nums text-fg/80">
                {r.value}
              </span>
            </div>
          ))}
        </div>
      )}
      {props.children}
    </div>
  );
}

/** 最高柱 84% 高、單柱最寬 36px、按本機工具主題色堆疊。 */
export function TrendChart(props: {
  rows: TrendAppRow[];
  stepMs: number;
  fillToday: boolean;
  hourly: boolean;
}) {
  const { buckets, apps, totals } = useMemo(
    () => fillAppBuckets(props.rows, props.stepMs, props.fillToday),
    [props.rows, props.stepMs, props.fillToday],
  );
  if (buckets.length === 0) {
    return <p className="text-sm text-fg/20">該範圍內暫無數據。</p>;
  }
  const max = Math.max(...buckets.map((b) => b.total), 1);
  const labelEvery = Math.max(1, Math.floor(buckets.length / 8));
  const spring = "duration-500 ease-[cubic-bezier(0.32,0.72,0,1)]";
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [hover, setHover] = useState<{ i: number; rect: DOMRect } | null>(null);
  const hb = hover != null ? buckets[hover.i] : null;
  return (
    <div className="anim-rise relative" ref={rootRef}>
      <div className="overflow-x-auto">
      <div className="min-w-0">
      <div className="chart-row flex h-36 items-end justify-center gap-1">
        {buckets.map((b, i) => {
          const h = b.total === 0 ? 3 : Math.max(5, (b.total / max) * 84);
          return (
            <div
              key={b.ts}
              onMouseEnter={(e) =>
                setHover({ i, rect: e.currentTarget.getBoundingClientRect() })
              }
              onMouseLeave={() => setHover(null)}
              className="chart-bar bar-grow flex w-full min-w-[4px] max-w-9 flex-1 flex-col justify-end overflow-hidden rounded-t border border-b-0 border-fg/10 bg-fg/[0.05] shadow-[inset_0_1px_0_rgba(255,255,255,0.18)]"
              style={{
                height: `${h}%`,
                animationDelay: `${Math.min(i * 12, 400)}ms`,
              }}
            >
              {b.segs.map((s) => {
                const c = appColor(s.app);
                return (
                  <div
                    key={s.app}
                    className={`w-full shrink-0 transition-[height] ${spring}`}
                    style={{
                      height: `${(s.tokens / b.total) * 100}%`,
                      background: `linear-gradient(180deg, color-mix(in srgb, ${c} 55%, white), ${c})`,
                      borderTop: "1px solid rgba(255,255,255,0.14)",
                      opacity: 0.9,
                    }}
                  />
                );
              })}
            </div>
          );
        })}
      </div>
      <HoverTip
        rect={hover?.rect ?? null}
        rootRef={rootRef}
        title={hb ? `${fmtBucket(hb.ts, props.hourly)} · ${fmtInt(hb.total)} tokens` : undefined}
        rows={
          hb
            ? [
                { label: "請求", value: fmtInt(hb.requests) },
                { label: "費用", value: `$${fmtCost(hb.cost_usd)}` },
                ...hb.segs.map((s) => ({
                  dot: appColor(s.app),
                  // 用 appLabel 而非原始 id —— 圖例與提示都要顯示「DeepSeek Harness」
                  // 這種給人看的名稱，不是內部的 `dsh`。
                  label: appLabel(s.app),
                  value: `${fmtInt(s.tokens)} · ${hb.total ? ((s.tokens / hb.total) * 100).toFixed(0) : 0}%`,
                })),
              ]
            : []
        }
      />
      {apps.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1">
          {apps.map((a) => (
            <span
              key={a}
              className="flex items-center gap-1.5 text-[10px] text-fg/40"
            >
              <span
                className="inline-block h-2 w-2 rounded-sm"
                style={{
                  background: appColor(a),
                  boxShadow: `0 0 6px 0 ${appColor(a)}`,
                }}
              />
              {appLabel(a)} · {fmtInt(totals[a])}
            </span>
          ))}
        </div>
      )}
      <div className="mt-1 flex justify-center gap-1 text-[10px] text-fg/25">
        {buckets.map((b, i) => (
          <div
            key={b.ts}
            className="w-full min-w-[4px] max-w-9 flex-1 truncate text-center"
          >
            {i % labelEvery === 0 ? fmtBucket(b.ts, props.hourly) : ""}
          </div>
        ))}
      </div>
      </div>
      </div>
    </div>
  );
}

export function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d
    .getSeconds()
    .toString()
    .padStart(2, "0")}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

// ── 歷史回填對話框與最近請求 ───────────────────────────────────────────────
// 已搬到 `usageLogs.tsx`（本檔 2026-09-28 時 546 行）。這裡原樣再匯出，
// 讓既有 `from "./usageCharts"` 的匯入端不必改路徑。
export { HistoryImportDialog, RecentLogs, timeAgo } from "./usageLogs";


// ── 用量日曆（GitHub 式週列） ────────────────────────────────────────────
