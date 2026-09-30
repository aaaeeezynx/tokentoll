//! 「工具」視角：分本機工具並排比較。
//!
//! 存在理由：其他視角都無法同時看到各工具的用量。`總覽` 一次只回一個聚合值
//! （要一個工具一個工具切），Token 趨勢圖只有 tokens 沒有快取拆分。
//! 這裡是全 App 唯一能並排比較「Token 用量／請求數／快取命中率」的地方。
//!
//! ⚠️ 整體命中率一律取自後端 `summary`，**不可**由本表各行平均。
//! 各工具 token 量差距極大（codex 12.2 億 vs hermes 2.5 萬），
//! 實測簡單平均 40.37% 與正確的 token 加權值 60.08% 相差 19.7 個百分點。

import {
  useMemo,
  useState,
} from "react";
import {
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  type AppStat,
  type UsageSummary,
} from "../../lib/api";
import {
  Logo,
  appLabel,
} from "../logos";
import {
  fmtCost,
  fmtInt,
  fmtPct,
} from "./usageTypes";

type SortKey =
  | "tokens"
  | "requests"
  | "cache_hit_rate"
  | "in_tok"
  | "out_tok"
  | "cache_read"
  | "cache_write"
  | "cost_usd"
  | "success_rate";

const COLUMNS: { key: SortKey; label: string; hint: string }[] = [
  { key: "requests", label: "請求數", hint: "該工具發出的請求總數" },
  { key: "success_rate", label: "成功率", hint: "HTTP 2xx 佔比" },
  { key: "in_tok", label: "輸入", hint: "未命中快取的輸入 tokens" },
  { key: "out_tok", label: "輸出", hint: "模型產生的 tokens" },
  { key: "cache_read", label: "快取讀", hint: "命中快取而省下的 tokens" },
  { key: "cache_write", label: "快取建", hint: "寫入快取的 tokens" },
  { key: "tokens", label: "總計 Tokens", hint: "輸入＋輸出＋快取讀＋快取建" },
  { key: "cache_hit_rate", label: "快取命中率", hint: "快取讀 ÷（輸入＋快取讀）" },
  { key: "cost_usd", label: "費用 $", hint: "依定價表計算" },
];

/** 快取命中率的視覺化長條，一眼看出高低。 */
function HitBar(props: { rate: number }) {
  const pct = Math.max(0, Math.min(1, props.rate)) * 100;
  return (
    <div className="flex items-center justify-end gap-1.5">
      <div className="h-1.5 w-10 overflow-hidden rounded-full bg-fg/[0.05]">
        <div
          className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="w-11 text-right tabular-nums">{fmtPct(props.rate)}</span>
    </div>
  );
}

export function AppsLens(props: {
  byApp: UseQueryResult<AppStat[], unknown>;
  summary: UseQueryResult<UsageSummary, unknown>;
  /** 點列下鑽：套用該工具到全域篩選。 */
  onPickApp: (app: string) => void;
}) {
  const data = props.byApp.data || [];
  const s = props.summary.data;
  const [sortKey, setSortKey] = useState<SortKey>("tokens");
  const [desc, setDesc] = useState(true);

  const sorted = useMemo(() => {
    const arr = [...data];
    arr.sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const d = typeof av === "number" && typeof bv === "number" ? av - bv : 0;
      return desc ? -d : d;
    });
    return arr;
  }, [data, sortKey, desc]);

  const grandTokens = Math.max(
    data.reduce((acc, r) => acc + r.tokens, 0),
    1,
  );

  const onSort = (k: SortKey) => {
    if (k === sortKey) {
      setDesc(!desc);
    } else {
      setSortKey(k);
      setDesc(true);
    }
  };

  return (
    <div className="min-w-0 space-y-4">
      {/* 整體摘要：一律取自後端 summary，避免與各列平均產生矛盾 */}
      {s && (
        <div className="glass min-w-0 p-4">
          <div className="mb-2.5 text-sm font-semibold tracking-tight text-fg/80">
            全部工具合計
          </div>
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            {[
              ["請求數", fmtInt(s.requests)],
              ["Token 用量", fmtInt(s.total_tokens)],
              ["快取命中率", fmtPct(s.cache_hit_rate)],
              ["費用 $", fmtCost(s.cost_usd)],
            ].map(([label, value]) => (
              <div key={label} className="min-w-0">
                <div className="text-[10px] text-fg/25">{label}</div>
                <div className="mt-0.5 truncate text-base font-semibold text-fg/85">
                  {value}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 各工具卡片：三個頭條數字一眼可見 */}
      <div className="grid min-w-0 gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {sorted.map((r) => (
          <button
            key={r.app}
            type="button"
            onClick={() => props.onPickApp(r.app)}
            title={`只看 ${appLabel(r.app)} 的用量`}
            className="glass min-w-0 p-4 text-left transition-colors hover:bg-fg/[0.055]"
          >
            <div className="flex min-w-0 items-center gap-2">
              <Logo name={r.app} size={18} className="shrink-0" />
              <span className="min-w-0 truncate text-[13px] font-semibold text-fg/85">
                {appLabel(r.app)}
              </span>
              <span className="ml-auto shrink-0 text-[10px] text-fg/30">
                {fmtPct(r.tokens / grandTokens)}
              </span>
            </div>
            <div className="mt-2.5 space-y-1.5">
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-[10px] text-fg/30">Token 用量</span>
                <span className="truncate font-mono text-[13px] font-medium text-fg/80">
                  {fmtInt(r.tokens)}
                </span>
              </div>
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-[10px] text-fg/30">請求數</span>
                <span className="font-mono text-[13px] font-medium text-fg/80">
                  {fmtInt(r.requests)}
                </span>
              </div>
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-[10px] text-fg/30">快取命中率</span>
                <span className="font-mono text-[13px] font-medium text-fg/80">
                  {fmtPct(r.cache_hit_rate)}
                </span>
              </div>
            </div>
            <div className="mt-2.5 h-1 w-full overflow-hidden rounded-full bg-fg/[0.05]">
              <div
                className="h-full rounded-full bar-anim"
                style={{
                  width: `${(r.tokens / grandTokens) * 100}%`,
                  background: "var(--mac-accent)",
                  opacity: 0.45,
                }}
              />
            </div>
          </button>
        ))}
        {data.length === 0 && (
          <p className="text-xs text-fg/20">此區間暫無數據</p>
        )}
      </div>

      {/* 並排比較表 */}
      <div className="glass min-w-0 p-5">
        <div className="mb-1 text-sm font-semibold tracking-tight text-fg/80">
          分工具比較
        </div>
        <div className="mb-3 text-[10px] text-fg/25">
          點欄位標題可排序；點任一列可只看該工具
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[820px] text-left text-xs">
            <thead>
              <tr className="text-fg/55">
                <th className="py-1.5 pr-2">工具</th>
                {COLUMNS.map((c) => (
                  <th
                    key={c.key}
                    title={c.hint}
                    onClick={() => onSort(c.key)}
                    className="cursor-pointer py-1.5 pr-2 text-right whitespace-nowrap select-none hover:text-fg/80"
                  >
                    {c.label}
                    {sortKey === c.key ? (desc ? " ↓" : " ↑") : ""}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {sorted.map((r) => (
                <tr
                  key={r.app}
                  onClick={() => props.onPickApp(r.app)}
                  title={`只看 ${appLabel(r.app)} 的用量`}
                  className="cursor-pointer border-t border-fg/[0.04] transition-colors hover:bg-fg/[0.04]"
                >
                  <td className="py-2 pr-2">
                    <div className="flex items-center gap-1.5">
                      <Logo name={r.app} size={14} className="shrink-0" />
                      <span className="text-fg/70">{appLabel(r.app)}</span>
                    </div>
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtInt(r.requests)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtPct(r.success_rate)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtInt(r.in_tok)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtInt(r.out_tok)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtInt(r.cache_read)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtInt(r.cache_write)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/80">
                    {fmtInt(r.tokens)}
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/80">
                    <HitBar rate={r.cache_hit_rate} />
                  </td>
                  <td className="py-2 pr-2 text-right whitespace-nowrap tabular-nums text-fg/60">
                    {fmtCost(r.cost_usd)}
                  </td>
                </tr>
              ))}
              {data.length === 0 && (
                <tr>
                  <td className="py-2 text-fg/20" colSpan={COLUMNS.length + 1}>
                    此區間暫無數據
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
