//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import {
  type TrendBucket,
} from "../../lib/api";
import {
  Segmented,
} from "../Segmented";
import {
  HoverTip,
} from "./usageCharts";
import {
  fmtCost,
  fmtInt,
} from "./usageTypes";
import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export type GridRange = "30d" | "90d" | "180d" | "365d";
export const GRID_RANGES: {
  id: GridRange;
  label: string;
  days: number;
  cols: number;
  cell?: number;
}[] = [
  { id: "30d", label: "月", days: 30, cols: 10, cell: 34 },
  { id: "90d", label: "季", days: 90, cols: 30 },
  { id: "180d", label: "半年", days: 180, cols: 45 },
  { id: "365d", label: "年", days: 365, cols: 61 },
];

export type DaySlot = {
  key: string;
  label: string;
  tokens: number;
  requests: number;
  cost: number;
  future?: boolean;
};

export function DayCell(props: {
  d: DaySlot;
  s: number;
  rad: number;
  color: string;
  delay: number;
  onTip?: (d: DaySlot, rect: DOMRect) => void;
  onTipEnd?: () => void;
}) {
  const { d } = props;
  return (
    <div
      onMouseEnter={(e) =>
        props.onTip?.(d, e.currentTarget.getBoundingClientRect())
      }
      onMouseLeave={() => props.onTipEnd?.()}
      className="day-cell shrink-0 anim-rise"
      style={{
        width: props.s,
        height: props.s,
        borderRadius: props.rad,
        backgroundColor: props.color,
        animationDelay: `${props.delay}ms`,
      }}
    />
  );
}

export function ContributionGrid(props: {
  data: TrendBucket[];
  range: GridRange;
  onRange: (r: GridRange) => void;
}) {
  const def = GRID_RANGES.find((g) => g.id === props.range)!;
  // 量測可用寬度：格子按檔位縮放，寬度填滿、高度收斂。
  // callback ref：換檔重掛時也會重新綁定 observer。
  const [wrapEl, setWrapEl] = useState<HTMLDivElement | null>(null);
  const [wrapW, setWrapW] = useState(900);
  useLayoutEffect(() => {
    if (!wrapEl) return;
    const ro = new ResizeObserver((es) => {
      const w = es[0]?.contentRect.width;
      if (w && w > 0) setWrapW(w);
    });
    ro.observe(wrapEl);
    return () => ro.disconnect();
  }, [wrapEl]);
  // 窄窗：月視圖收起左右裝飾面板，中央單面板流式填滿（經典三面板需 ~340px+）
  const compact = wrapW < 560;
  const perCol = wrapW / def.cols;
  const gap = perCol > 28 ? 6 : perCol > 16 ? 4 : 3;
  const s =
    !compact && def.cell
      ? def.cell
      : Math.max(6, Math.floor((wrapW - (def.cols - 1) * gap) / def.cols));
  const rad = Math.min(10, Math.max(3, Math.round(s * 0.2)));
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [tip, setTip] = useState<{ d: DaySlot; rect: DOMRect } | null>(null);
  const cellHandlers = {
    onTip: (d: DaySlot, rect: DOMRect) => setTip({ d, rect }),
    onTipEnd: () => setTip(null),
  };

  const { centerRows, prevRows, nextRows, total, maxTokens } = useMemo(() => {
    const byDay = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of props.data) {
      const d = new Date(b.ts);
      const k = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      const e = byDay.get(k) ?? { tokens: 0, cost: 0, requests: 0 };
      e.tokens += b.tokens;
      e.cost += b.cost_usd;
      e.requests += b.requests;
      byDay.set(k, e);
    }
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    const t0 = midnight.getTime();
    type Slot = {
      key: string;
      label: string;
      tokens: number;
      cost: number;
      requests: number;
      future?: boolean;
    };
    // 月檔多建前 30 天（左側上月真實數據）
    const needDays = props.range === "30d" ? 60 : def.days;
    const slots: Slot[] = [];
    for (let i = needDays - 1; i >= 0; i--) {
      const d = new Date(t0 - i * 86400000);
      const k = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      const v = byDay.get(k);
      slots.push({
        key: k,
        label: `${d.getMonth() + 1}/${d.getDate()}`,
        tokens: v?.tokens ?? 0,
        cost: v?.cost ?? 0,
        requests: v?.requests ?? 0,
      });
    }
    const center =
      props.range === "30d" ? slots.slice(-30) : slots;
    const prev = props.range === "30d" ? slots.slice(0, 30) : [];
    // 下月：未來佔位（無數據，純裝飾）
    const next: Slot[] = [];
    if (props.range === "30d") {
      for (let i = 1; i <= 30; i++) {
        const d = new Date(t0 + i * 86400000);
        next.push({
          key: `f${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`,
          label: `${d.getMonth() + 1}/${d.getDate()}`,
          tokens: 0,
          cost: 0,
          requests: 0,
          future: true,
        });
      }
    }
    let total = 0;
    let maxTokens = 1;
    for (const sl of center) {
      total += sl.tokens;
      if (sl.tokens > maxTokens) maxTokens = sl.tokens;
    }
    const chunk = (arr: Slot[], n: number): Slot[][] => {
      const out: Slot[][] = [];
      for (let r = 0; r < arr.length; r += n) out.push(arr.slice(r, r + n));
      return out;
    };
    return {
      centerRows: chunk(center, def.cols),
      prevRows: chunk(prev, 10),
      nextRows: chunk(next, 10),
      total,
      maxTokens,
    };
  }, [props.data, props.range, def.days, def.cols]);

  const color = (tokens: number) => {
    if (tokens === 0) return "rgba(255,255,255,0.04)";
    const ratio = tokens / maxTokens;
    if (ratio > 0.75) return "color-mix(in srgb, var(--mac-accent) 85%, transparent)";
    if (ratio > 0.5) return "color-mix(in srgb, var(--mac-accent) 62%, transparent)";
    if (ratio > 0.25) return "color-mix(in srgb, var(--mac-accent) 40%, transparent)";
    return "color-mix(in srgb, var(--mac-accent) 20%, transparent)";
  };

  return (
    <div className="glass relative min-w-0 p-4" ref={rootRef}>
      <div className="mb-2.5 flex min-w-0 flex-wrap items-center gap-2">
        <div className="shrink-0 text-sm font-semibold tracking-tight text-white/80">
          用量日曆
        </div>
        <span className="text-[11px] text-white/45">
          共 {fmtInt(total)} tokens
        </span>
        <span className="flex-1" />
      </div>
      <div className="mb-2.5">
        <Segmented
          className="w-full"
          value={props.range}
          onChange={(v) => props.onRange(v as GridRange)}
          btnClassName="py-1 text-[11px] font-medium"
          options={GRID_RANGES.map((g) => ({ id: g.id, label: g.label }))}
        />
      </div>
      {/* 內襯留白：hover 放大/光暈不觸發滾動條 */}
      <div ref={setWrapEl} key={props.range} className="overflow-x-auto px-2 py-2.5">
        {props.range === "30d" && !compact ? (
          <div className="flex min-w-0 items-center gap-3 md:gap-5">
            <div className="flex-1 overflow-hidden brightness-[.5]">
              <div className="flex flex-col items-end" style={{ gap }}>
                {prevRows.map((row, ri) => (
                  <div key={ri} className="flex justify-end" style={{ gap }}>
                    {row.map((d, i) => (
                      <DayCell
                        key={d.key}
                        d={d}
                        s={s}
                        rad={rad}
                        color={color(d.tokens)}
                        delay={Math.min((ri * row.length + i) * 1.5, 300)}
                        {...cellHandlers}
                      />
                    ))}
                  </div>
                ))}
              </div>
            </div>
            <div className="flex shrink-0 flex-col" style={{ gap }}>
              {centerRows.map((row, ri) => (
                <div key={ri} className="flex" style={{ gap }}>
                  {row.map((d, i) => (
                    <DayCell
                      key={d.key}
                      d={d}
                      s={s}
                      rad={rad}
                      color={color(d.tokens)}
                      delay={Math.min((ri * row.length + i) * 1.5, 300)}
                      {...cellHandlers}
                    />
                  ))}
                </div>
              ))}
            </div>
            <div className="flex-1 overflow-hidden brightness-[.5]">
              <div className="flex flex-col items-start" style={{ gap }}>
                {nextRows.map((row, ri) => (
                  <div key={ri} className="flex justify-start" style={{ gap }}>
                    {row.map((d, i) => (
                      <DayCell
                        key={d.key}
                        d={d}
                        s={s}
                        rad={rad}
                        color={color(d.tokens)}
                        delay={Math.min((ri * row.length + i) * 1.5, 300)}
                        {...cellHandlers}
                      />
                    ))}
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="mx-auto flex w-max min-w-full flex-col items-center" style={{ gap }}>
            {centerRows.map((row, ri) => (
              <div key={ri} className="flex" style={{ gap }}>
                {row.map((d, i) => (
                  <DayCell
                    key={d.key}
                    d={d}
                    s={s}
                    rad={rad}
                    color={color(d.tokens)}
                    delay={Math.min((ri * row.length + i) * 1.5, 300)}
                    {...cellHandlers}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="mt-1.5 flex items-center gap-1 text-[10px] text-white/35">
        <span>少</span>
        {[0.2, 0.4, 0.62, 0.85].map((op) => (
          <div
            key={op}
            className="h-2.5 w-2.5 rounded-[3px]"
            style={{ backgroundColor: `color-mix(in srgb, var(--mac-accent) ${op * 100}%, transparent)` }}
          />
        ))}
        <span>多</span>
      </div>
      <HoverTip
        rect={tip?.rect ?? null}
        rootRef={rootRef}
        title={
          tip
            ? `${tip.d.label}${tip.d.future ? " · 未來" : ""}`
            : undefined
        }
        rows={
          tip && !tip.d.future
            ? [
                { label: "Tokens", value: fmtInt(tip.d.tokens) },
                { label: "請求", value: fmtInt(tip.d.requests) },
                { label: "費用", value: `$${fmtCost(tip.d.cost)}` },
              ]
            : []
        }
      >
        {tip && !tip.d.future && tip.d.tokens > 0 && (
          <div className="mt-1.5 flex items-center gap-2">
            <div className="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[0.07]">
              <div
                className="h-full rounded-full"
                style={{
                  width: `${Math.max(3, (tip.d.tokens / maxTokens) * 100)}%`,
                  background: "var(--mac-accent)",
                }}
              />
            </div>
            <span className="shrink-0 text-[10px] text-white/30">
              {((tip.d.tokens / maxTokens) * 100).toFixed(0)}% 峰值
            </span>
          </div>
        )}
      </HoverTip>
    </div>
  );
}

// ── 子視角：總覽 ─────────────────────────────────────────────────────────

// ── 訂閱額度（OpenCode Go / Claude / ChatGPT）：官方額度百分比 + 重置倒計時 ──
