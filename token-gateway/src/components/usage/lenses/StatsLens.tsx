//! 由 `usageLenses.tsx` 拆出（原檔 453 行）。程式碼語意未改，只搬位置。

import {
  type ModelStat,
  type ProviderStat,
  type UsageSummary,
} from "../../../lib/api";
import { fmtCost, fmtInt } from "../usageTypes";
import { type UseQueryResult } from "@tanstack/react-query";

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
        <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">摘要</div>
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
                <div className="text-[10px] text-fg/25">{label}</div>
                <div className="text-sm font-medium text-fg/70">{value}</div>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-fg/30">載入中…</p>
        )}
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-fg/35">最常使用模型</div>
          <div className="mt-1.5 text-sm font-semibold text-fg/80">
            {topModel ? topModel.model : "—"}
          </div>
          <div className="text-[10px] text-fg/25">
            {topModel ? `${fmtInt(topModel.tokens)} tokens · $${fmtCost(topModel.cost_usd)}` : ""}
          </div>
        </div>
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-fg/35">最常用來源</div>
          <div className="mt-1.5 text-sm font-semibold text-fg/80">
            {topProvider ? topProvider.provider_name : "—"}
          </div>
          <div className="text-[10px] text-fg/25">
            {topProvider ? `${fmtInt(topProvider.tokens)} tokens · $${fmtCost(topProvider.cost_usd)}` : ""}
          </div>
        </div>
      </div>
    </div>
  );
}
