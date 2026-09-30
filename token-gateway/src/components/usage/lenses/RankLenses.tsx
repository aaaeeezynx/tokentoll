//! 由 `usageLenses.tsx` 拆出（原檔 453 行）。程式碼語意未改，只搬位置。

import {
  type ModelStat,
  type ProviderStat,
} from "../../../lib/api";
import { HoverTip } from "../usageCharts";
import { fmtCost, fmtInt, fmtPct } from "../usageTypes";
import { type UseQueryResult } from "@tanstack/react-query";

// ── 子視角：模型排行 ──────────────────────────────────────────────────────

export function ModelsLens(props: {
  byModel: UseQueryResult<ModelStat[], unknown>;
}) {
  const data = props.byModel.data || [];
  const maxCost = Math.max(...data.map((m) => m.cost_usd), 0.001);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">模型排行（按費用）</div>
      <div className="space-y-1.5">
        {data.map((m) => (
          <div key={m.model} className="flex items-center gap-3 rounded-xl bg-fg/[0.03] px-3.5 py-2.5 transition-colors hover:bg-fg/[0.055]">
            <div className="min-w-0 flex-1">
              <div className="truncate font-mono text-xs text-fg/70">{m.model}</div>
              <div className="text-[10px] text-fg/25">
                {fmtInt(m.requests)} 請求 · 輸入 {fmtInt(m.in_tok)} · 輸出 {fmtInt(m.out_tok)} · 快取讀{" "}
                {fmtInt(m.cache_read)} · 快取建 {fmtInt(m.cache_write)} · 總計 {fmtInt(m.tokens)}
              </div>
            </div>
            <div className="shrink-0 text-right">
              <div className="text-xs font-medium text-fg/60">${fmtCost(m.cost_usd)}</div>
              <div className="text-[10px] text-fg/35">命中 {fmtPct(m.cache_hit_rate)}</div>
              <div className="mt-0.5 h-1 w-16 overflow-hidden rounded-full bg-fg/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(m.cost_usd / maxCost) * 100}%` }}
                />
              </div>
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-fg/20">暫無數據</p>}
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
  const totalTokens = data.reduce((s, x) => s + x.tokens, 0) || 1;
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">來源統計</div>
      <div className="overflow-x-auto">
      <table className="w-full min-w-[760px] text-left text-xs">
        <thead>
          <tr className="text-fg/55">
            <th className="py-1.5 pr-2">來源</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">請求</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">成功率</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">輸入</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">輸出</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">快取讀</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">快取建</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">Tokens</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">快取命中率</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">費用$</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">佔比</th>
          </tr>
        </thead>
        <tbody>
          {data.map((p) => (
            <tr key={p.provider_id ?? -1} className="border-t border-fg/[0.04]">
              <td className="py-2 pr-2 text-fg/70">{p.provider_name}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.requests)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtPct(p.success_rate)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.in_tok)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.out_tok)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.cache_read)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.cache_write)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtInt(p.tokens)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/80">{fmtPct(p.cache_hit_rate)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">{fmtCost(p.cost_usd)}</td>
              <td className="py-2 pr-2 text-right">
                <div className="flex items-center justify-end gap-1.5">
                  <div className="h-1.5 w-12 overflow-hidden rounded-full bg-fg/[0.05]">
                    <div
                      className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                      style={{ width: `${(p.tokens / maxTokens) * 100}%` }}
                    />
                  </div>
                  <span className="text-[10px] text-fg/30">{fmtPct(p.tokens / totalTokens)}</span>
                </div>
              </td>
            </tr>
          ))}
          {data.length === 0 && (
            <tr><td className="py-2 text-fg/20" colSpan={11}>暫無數據</td></tr>
          )}
        </tbody>
      </table>
      </div>
    </div>
  );
}

export { HoverTip };
