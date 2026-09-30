//! 由 `usageLenses.tsx` 拆出（原檔 453 行）。程式碼語意未改，只搬位置。

import {
  type HourlyBucket,
  type TrendBucket,
  type UsageRange,
} from "../../../lib/api";
import { HoverTip } from "../usageCharts";
import { fmtCost, fmtInt } from "../usageTypes";
import { type UseQueryResult } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";

// ── 子視角：月聚合 ────────────────────────────────────────────────────────

export function MonthlyLens(props: {
  trend: UseQueryResult<TrendBucket[], unknown>;
  range: UsageRange;
}) {
  const data = useMemo(() => {
    const buckets = props.trend.data || [];
    const byMonth = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of buckets) {
      const d = new Date(b.ts);
      const key = `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, "0")}`;
      const existing = byMonth.get(key) ?? { tokens: 0, cost: 0, requests: 0 };
      existing.tokens += b.tokens;
      existing.cost += b.cost_usd;
      existing.requests += b.requests;
      byMonth.set(key, existing);
    }
    return Array.from(byMonth.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([month, v]) => ({ month, ...v }));
  }, [props.trend.data]);

  const maxTokens = Math.max(...data.map((d) => d.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">月度用量</div>
      <div className="space-y-1.5">
        {data.map((d) => (
          <div key={d.month} className="flex items-center gap-3 rounded-xl bg-fg/[0.03] px-3.5 py-2.5 transition-colors hover:bg-fg/[0.055]">
            <div className="w-16 text-xs font-medium text-fg/50">{d.month}</div>
            <div className="flex-1">
              <div className="h-4 overflow-hidden rounded-full bg-fg/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-fg/35">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-fg/20">暫無數據</p>}
      </div>
    </div>
  );
}

// ── 子視角：日聚合 ────────────────────────────────────────────────────────

export function DailyLens(props: {
  trend: UseQueryResult<TrendBucket[], unknown>;
}) {
  const data = useMemo(() => {
    const buckets = props.trend.data || [];
    const byDay = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of buckets) {
      const d = new Date(b.ts);
      const key = `${d.getMonth() + 1}/${d.getDate()}`;
      const existing = byDay.get(key) ?? { tokens: 0, cost: 0, requests: 0 };
      existing.tokens += b.tokens;
      existing.cost += b.cost_usd;
      existing.requests += b.requests;
      byDay.set(key, existing);
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => {
        const [am, ad] = a.split("/").map(Number);
        const [bm, bd] = b.split("/").map(Number);
        return am !== bm ? am - bm : ad - bd;
      })
      .map(([day, v]) => ({ day, ...v }));
  }, [props.trend.data]);

  const maxTokens = Math.max(...data.map((d) => d.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">每日用量</div>
      <div className="space-y-1">
        {data.map((d) => (
          <div key={d.day} className="flex items-center gap-3 rounded-xl bg-fg/[0.03] px-3.5 py-2">
            <div className="w-12 text-[10px] font-medium text-fg/40">{d.day}</div>
            <div className="flex-1">
              <div className="h-3 overflow-hidden rounded-full bg-fg/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-fg/30">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-fg/20">暫無數據</p>}
      </div>
    </div>
  );
}

// ── 子視角：時段熱力圖 ──────────────────────────────────────────────────

export function HourlyLens(props: {
  hourly: UseQueryResult<HourlyBucket[], unknown>;
}) {
  const data = props.hourly.data || [];
  // 填補 0-23 所有小時
  const filled = Array.from({ length: 24 }, (_, i) => {
    const found = data.find((d) => d.hour === i);
    return found ?? { hour: i, requests: 0, tokens: 0, cost_usd: 0 };
  });
  const maxTokens = Math.max(...filled.map((d) => d.tokens), 1);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [hover, setHover] = useState<{ i: number; rect: DOMRect } | null>(null);
  const hd = hover != null ? filled[hover.i] : null;
  return (
    <div className="glass relative p-5" ref={rootRef}>
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">時段分佈（哪個時段最燒）</div>
      <div className="chart-row flex h-32 items-end gap-0.5">
        {filled.map((d, i) => (
          <div
            key={d.hour}
            onMouseEnter={(e) =>
              setHover({ i, rect: e.currentTarget.getBoundingClientRect() })
            }
            onMouseLeave={() => setHover(null)}
            className="chart-bar bar-grow flex-1 rounded-t"
            style={{
              height: `${Math.max(2, (d.tokens / maxTokens) * 100)}%`,
              animationDelay: `${d.hour * 15}ms`,
              backgroundColor:
                d.tokens === 0
                  ? "rgba(255,255,255,0.03)"
                  : d.tokens / maxTokens > 0.7
                    ? "color-mix(in srgb, var(--mac-accent) 58%, transparent)"
                    : d.tokens / maxTokens > 0.3
                      ? "color-mix(in srgb, var(--mac-accent) 38%, transparent)"
                      : "color-mix(in srgb, var(--mac-accent) 20%, transparent)",
            }}
          />
        ))}
      </div>
      <div className="mt-1 flex gap-0.5 text-[10px] text-fg/25">
        {filled.map((d) => (
          <div key={d.hour} className="flex-1 text-center">
            {d.hour % 3 === 0 ? `${d.hour}` : ""}
          </div>
        ))}
      </div>
      <HoverTip
        rect={hover?.rect ?? null}
        rootRef={rootRef}
        title={hd ? `${hd.hour}:00 - ${hd.hour + 1}:00` : undefined}
        rows={
          hd
            ? [
                { label: "Tokens", value: fmtInt(hd.tokens) },
                { label: "請求", value: fmtInt(hd.requests) },
                { label: "費用", value: `$${fmtCost(hd.cost_usd)}` },
              ]
            : []
        }
      />
    </div>
  );
}
