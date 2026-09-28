//! 由 `usageLenses.tsx` 拆出（原檔 453 行）。程式碼語意未改，只搬位置。

import {
  type SubscriptionFees,
  type TrendAppRow,
  type TrendBucket,
  type UsageSummary,
} from "../../../lib/api";
import { ContributionGrid, GridRange } from "../usageActivity";
import { TrendChart } from "../usageCharts";
import { QuotaSection } from "../usageQuota";
import { fmtCost, fmtInt, fmtPct } from "../usageTypes";
import { type UseQueryResult } from "@tanstack/react-query";

/** 總覽：摘要卡、訂閱費、貢獻格子、Token 趨勢。 */
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
