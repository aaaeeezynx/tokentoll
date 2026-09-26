//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import {
  type HourlyBucket,
  type ModelStat,
  type ProviderStat,
  type SubscriptionFees,
  type TrendAppRow,
  type TrendBucket,
  type UsageRange,
  type UsageSummary,
} from "../../lib/api";
import {
  ContributionGrid,
  GridRange,
} from "./usageActivity";
import {
  HoverTip,
  TrendChart,
} from "./usageCharts";
import {
  QuotaSection,
} from "./usageQuota";
import {
  fmtCost,
  fmtInt,
  fmtPct,
} from "./usageTypes";
import {
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  useMemo,
  useRef,
  useState,
} from "react";

export function OverviewLens(props: {
  summary: UseQueryResult<UsageSummary, unknown>;
  trendByApp: UseQueryResult<TrendAppRow[], unknown>;
  chartKey: string;
  stepMs: number;
  hourly: boolean;
  gridTrend: UseQueryResult<TrendBucket[], unknown>;
  gridRange: GridRange;
  onGridRange: (r: GridRange) => void;
  subFees: UseQueryResult<SubscriptionFees, unknown>;
}) {
  const { summary, trendByApp, chartKey, stepMs, hourly, gridTrend, gridRange, onGridRange, subFees } = props;
  const subTotal = subFees.data?.total_fee ?? 0;
  const grandTotal = (summary.data?.cost_usd ?? 0) + subTotal;
  return (
    <div className="min-w-0 space-y-4">
      {summary.data && (
        <div className="grid min-w-0 grid-cols-2 gap-3 lg:grid-cols-4">
          {[
            ["總請求", fmtInt(summary.data.requests)],
            ["真實消耗 Tokens", fmtInt(summary.data.total_tokens)],
            ["快取命中率", fmtPct(summary.data.cache_hit_rate)],
            ["請求費用 $", fmtCost(summary.data.cost_usd)],
            ["成功率", fmtPct(summary.data.success_rate)],
            ["本月訂閱費 $", fmtCost(subTotal)],
            ["合計成本 $", fmtCost(grandTotal)],
          ].map(([label, value], i) => (
            <div
              key={label}
              className="glass-float float-hover anim-rise min-w-0 p-4"
              style={{ animationDelay: `${i * 50}ms` }}
            >
              <div className="text-[11px] font-medium text-white/35 uppercase tracking-wider">{label}</div>
              <div className="mt-1.5 truncate text-xl font-semibold text-white/90">{value}</div>
            </div>
          ))}
        </div>
      )}
      {summary.data && subTotal > 0 && (
        <p className="-mt-2 text-[11px] text-white/25">
          合計＝請求費用（當前篩選）＋本月訂閱費（自然月）；超額部分已按基礎價計入請求費用
        </p>
      )}

      <QuotaSection />

      <ContributionGrid
        data={gridTrend.data || []}
        range={gridRange}
        onRange={onGridRange}
      />

      <div className="glass min-w-0 p-3 md:p-5">
        <div className="mb-2 flex min-w-0 flex-wrap items-baseline gap-x-2 text-sm font-semibold tracking-tight text-white/80">
          <span className="shrink-0">Token 趨勢</span>
          {summary.data && (
            <span className="min-w-0 text-xs font-normal break-words text-white/30">
              輸入 {fmtInt(summary.data.in_tok)} · 輸出{" "}
              {fmtInt(summary.data.out_tok)} · 快取讀{" "}
              {fmtInt(summary.data.cache_read)} · 快取建{" "}
              {fmtInt(summary.data.cache_write)}
            </span>
          )}
        </div>
        {trendByApp.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : (
          <TrendChart
            key={chartKey}
            rows={trendByApp.data || []}
            stepMs={stepMs}
            fillToday={chartKey === "today"}
            hourly={hourly}
          />
        )}
      </div>
    </div>
  );
}

// ── 子視角：模型排行 ──────────────────────────────────────────────────────

export function ModelsLens(props: {
  byModel: UseQueryResult<ModelStat[], unknown>;
}) {
  const data = props.byModel.data || [];
  const maxCost = Math.max(...data.map((m) => m.cost_usd), 0.001);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">模型排行（按費用）</div>
      <div className="space-y-1.5">
        {data.map((m) => (
          <div key={m.model} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2.5 transition-colors hover:bg-white/[0.055]">
            <div className="min-w-0 flex-1">
              <div className="truncate font-mono text-xs text-white/70">{m.model}</div>
              <div className="text-[10px] text-white/25">
                {fmtInt(m.requests)} 請求 · 輸入 {fmtInt(m.in_tok)} · 輸出 {fmtInt(m.out_tok)}
              </div>
            </div>
            <div className="text-right">
              <div className="text-xs font-medium text-white/60">${fmtCost(m.cost_usd)}</div>
              <div className="mt-0.5 h-1 w-16 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(m.cost_usd / maxCost) * 100}%` }}
                />
              </div>
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
      </div>
    </div>
  );
}

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
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">月度用量</div>
      <div className="space-y-1.5">
        {data.map((d) => (
          <div key={d.month} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2.5 transition-colors hover:bg-white/[0.055]">
            <div className="w-16 text-xs font-medium text-white/50">{d.month}</div>
            <div className="flex-1">
              <div className="h-4 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-white/35">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
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
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">每日用量</div>
      <div className="space-y-1">
        {data.map((d) => (
          <div key={d.day} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2">
            <div className="w-12 text-[10px] font-medium text-white/40">{d.day}</div>
            <div className="flex-1">
              <div className="h-3 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-white/30">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
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
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">時段分佈（哪個時段最燒）</div>
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
      <div className="mt-1 flex gap-0.5 text-[10px] text-white/25">
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

// ── 子視角：統計 ─────────────────────────────────────────────────────────

export function StatsLens(props: {
  summary: UseQueryResult<UsageSummary, unknown>;
  byModel: UseQueryResult<ModelStat[], unknown>;
  byProvider: UseQueryResult<ProviderStat[], unknown>;
}) {
  const s = props.summary.data;
  const models = props.byModel.data || [];
  const providers = props.byProvider.data || [];
  const topModel = models[0];
  const topProvider = providers[0];
  return (
    <div className="space-y-4">
      <div className="glass p-5">
        <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">摘要</div>
        {s ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
            {[
              ["總請求", fmtInt(s.requests)],
              ["成功請求", fmtInt(s.ok_requests)],
              ["失敗請求", fmtInt(s.requests - s.ok_requests)],
              ["總 Tokens", fmtInt(s.total_tokens)],
              ["輸入 Tokens", fmtInt(s.in_tok)],
              ["輸出 Tokens", fmtInt(s.out_tok)],
              ["快取讀", fmtInt(s.cache_read)],
              ["快取建", fmtInt(s.cache_write)],
              ["總費用 $", fmtCost(s.cost_usd)],
            ].map(([label, value]) => (
              <div key={label}>
                <div className="text-[10px] text-white/25">{label}</div>
                <div className="text-sm font-medium text-white/70">{value}</div>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-white/30">載入中…</p>
        )}
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-white/35">最常使用模型</div>
          <div className="mt-1.5 text-sm font-semibold text-white/80">
            {topModel ? topModel.model : "—"}
          </div>
          <div className="text-[10px] text-white/25">
            {topModel ? `${fmtInt(topModel.tokens)} tokens · $${fmtCost(topModel.cost_usd)}` : ""}
          </div>
        </div>
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-white/35">最常用來源</div>
          <div className="mt-1.5 text-sm font-semibold text-white/80">
            {topProvider ? topProvider.provider_name : "—"}
          </div>
          <div className="text-[10px] text-white/25">
            {topProvider ? `${fmtInt(topProvider.tokens)} tokens · $${fmtCost(topProvider.cost_usd)}` : ""}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── 子視角：來源 ─────────────────────────────────────────────────────────

export function ChannelsLens(props: {
  byProvider: UseQueryResult<ProviderStat[], unknown>;
}) {
  const data = props.byProvider.data || [];
  const maxTokens = Math.max(...data.map((p) => p.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">來源統計</div>
      <div className="overflow-x-auto">
      <table className="w-full min-w-[480px] text-left text-xs">
        <thead>
          <tr className="text-white/55">
            <th className="py-1.5 pr-2">來源</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">請求</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">Tokens</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">費用$</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">佔比</th>
          </tr>
        </thead>
        <tbody>
          {data.map((p) => (
            <tr key={p.provider_id ?? -1} className="border-t border-white/[0.04]">
              <td className="py-2 pr-2 text-white/70">{p.provider_name}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{p.requests}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{fmtInt(p.tokens)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{fmtCost(p.cost_usd)}</td>
              <td className="py-2 pr-2 text-right">
                <div className="flex items-center justify-end gap-1.5">
                  <div className="h-1.5 w-12 overflow-hidden rounded-full bg-white/[0.05]">
                    <div
                      className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                      style={{ width: `${(p.tokens / maxTokens) * 100}%` }}
                    />
                  </div>
                  <span className="text-[10px] text-white/30">{fmtPct(p.tokens / (data.reduce((s, x) => s + x.tokens, 0) || 1))}</span>
                </div>
              </td>
            </tr>
          ))}
          {data.length === 0 && (
            <tr><td className="py-2 text-white/20" colSpan={5}>暫無數據</td></tr>
          )}
        </tbody>
      </table>
      </div>
    </div>
  );
}

// ── 篩選列（常駐頂部 toolbar，狀態由 Shell 持有） ─────────────────────────
