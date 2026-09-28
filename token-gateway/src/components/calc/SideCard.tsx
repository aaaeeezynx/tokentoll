import type { PriceQuote } from "../../lib/api";
import { fmtCost } from "./calcMath";

export function SideCard(props: {
  title: string;
  quote: PriceQuote | null;
  pending: boolean;
  winner: boolean;
  modelLabel: string;
}) {
  const q = props.quote;
  return (
    <div
      className={`glass-float p-4 ${props.winner ? "ring-1 ring-[#30d158]/40" : ""}`}
    >
      <div className="flex items-center gap-2">
        <div className="text-sm font-semibold tracking-tight text-white/85">
          {props.title}
        </div>
        {props.winner && (
          <span className="rounded-full bg-[#30d158]/15 px-2 py-0.5 text-[10px] font-medium text-[#30d158]">
            更便宜
          </span>
        )}
      </div>
      <div className="mt-0.5 truncate font-mono text-[11px] text-white/35">
        {props.modelLabel}
      </div>
      {props.pending ? (
        <p className="mt-2 text-sm text-white/30">試算中…</p>
      ) : q ? (
        <>
          <div className="mt-2 text-2xl font-semibold tracking-tight text-white">
            ${fmtCost(q.cost_usd)}
          </div>
          {q.mode === "subscription" ? (
            <div className="mt-2 space-y-1.5 text-xs text-white/45">
              <div className="flex justify-between">
                <span>月費</span>
                <span className="font-mono">${q.sub_fee_usd}</span>
              </div>
              <div>
                <div className="flex justify-between text-[11px]">
                  <span>額度</span>
                  <span className="font-mono">
                    {q.sub_used_this_month.toLocaleString()} /{" "}
                    {q.sub_included_tokens < 0
                      ? "無限"
                      : q.sub_included_tokens.toLocaleString()}
                  </span>
                </div>
                {q.sub_included_tokens >= 0 && (
                  <div className="mt-1 h-1 overflow-hidden rounded-full bg-white/[0.06]">
                    <div
                      className="h-full rounded-full bg-(--mac-accent)/60 bar-anim"
                      style={{
                        width: `${Math.min(100, (q.sub_used_this_month / Math.max(1, q.sub_included_tokens)) * 100)}%`,
                      }}
                    />
                  </div>
                )}
              </div>
              <div className="flex justify-between">
                <span>超額 {q.sub_over_tokens.toLocaleString()} tok</span>
                <span className="font-mono">
                  ${fmtCost(q.cost_usd - q.sub_fee_usd)}
                </span>
              </div>
              <div className="text-[11px] text-white/30">
                超額基礎價 {q.in_pm}/{q.out_pm} · 費率來源：{q.source}
              </div>
            </div>
          ) : (
            <div className="mt-2 space-y-1 text-xs text-white/45">
              <div className="flex justify-between">
                <span>輸入 ${q.in_pm}/M</span>
              </div>
              <div className="flex justify-between">
                <span>輸出 ${q.out_pm}/M</span>
              </div>
              <div className="flex justify-between">
                <span>
                  快取讀 ${q.cache_read_pm}/M · 建 ${q.cache_create_pm}/M
                </span>
              </div>
              <div className="text-[11px] text-white/30">費率來源：{q.source}</div>
            </div>
          )}
        </>
      ) : (
        <p className="mt-2 text-sm text-white/25">尚未試算</p>
      )}
    </div>
  );
}
