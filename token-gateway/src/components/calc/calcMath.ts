import type { PriceQuote, UsageSummary } from "../../lib/api";

export const SCENARIOS = [
  { id: "chat", label: "閒聊", inTok: 5000, outTok: 1000, cr: 0, cw: 0 },
  { id: "code", label: "程式", inTok: 50000, outTok: 10000, cr: 200000, cw: 5000 },
  { id: "doc", label: "長文", inTok: 200000, outTok: 5000, cr: 500000, cw: 0 },
  { id: "w7d", label: "近7天用量", inTok: 0, outTok: 0, cr: 0, cw: 0 },
  { id: "w30d", label: "近30天用量", inTok: 0, outTok: 0, cr: 0, cw: 0 },
  { id: "dates", label: "指定日期", inTok: 0, outTok: 0, cr: 0, cw: 0 },
] as const;

export function fillFromSummary(s: UsageSummary) {
  return { inTok: s.in_tok, outTok: s.out_tok, cr: s.cache_read, cw: s.cache_write };
}

export interface ManualRates {
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
}

export type Toks = { inTok: number; outTok: number; cr: number; cw: number };

export function manualQuote(r: ManualRates, t: Toks): PriceQuote {
  const cost =
    (t.inTok / 1e6) * r.in_pm +
    (t.outTok / 1e6) * r.out_pm +
    (t.cr / 1e6) * r.cache_read_pm +
    (t.cw / 1e6) * r.cache_create_pm;
  return {
    cost_usd: cost,
    in_pm: r.in_pm,
    out_pm: r.out_pm,
    cache_read_pm: r.cache_read_pm,
    cache_create_pm: r.cache_create_pm,
    source: "手填費率",
    mode: "usage",
    sub_fee_usd: 0,
    sub_included_tokens: -1,
    sub_used_this_month: 0,
    sub_over_tokens: 0,
  };
}

/** 手填訂閱試算（額度視為全新未用；抵扣順序同後端）。 */
export function manualSubQuote(
  fee: number,
  included: number,
  r: ManualRates,
  t: Toks,
): PriceQuote {
  let rem = included < 0 ? Number.MAX_SAFE_INTEGER : Math.max(0, included);
  let over = 0;
  let overTok = 0;
  const parts: [number, number][] = [
    [t.inTok, r.in_pm],
    [t.outTok, r.out_pm],
    [t.cr, r.cache_read_pm],
    [t.cw, r.cache_create_pm],
  ];
  for (const [amt, rate] of parts) {
    const a = Math.max(0, amt);
    const take = Math.min(rem, a);
    rem -= take;
    overTok += a - take;
    over += ((a - take) / 1e6) * rate;
  }
  return {
    cost_usd: fee + over,
    in_pm: r.in_pm,
    out_pm: r.out_pm,
    cache_read_pm: r.cache_read_pm,
    cache_create_pm: r.cache_create_pm,
    source: "手填費率",
    mode: "subscription",
    sub_fee_usd: fee,
    sub_included_tokens: included,
    sub_used_this_month: 0,
    sub_over_tokens: overTok,
  };
}

export function fmtCost(n: number): string {
  return n < 0.01 ? n.toFixed(6) : n.toFixed(4);
}
