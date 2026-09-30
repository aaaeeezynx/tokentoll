/**
 * 來源卡片底部的用量／餘額顯示（P2.2，對標 cc-switch 的卡片底部配額）。
 *
 * cc-switch 的規則照抄：**用量百分比 ＋ 重置倒數，顏色 <70% 綠 / 70–89% 橘 / ≥90% 紅**；
 * 多方案顯示方案數、可展開；卡片上有重新查詢的圖示。
 *
 * 差別：cc-switch 只對「目前使用中」的來源自動查（省額度），我們的自動查詢也照這個
 * 精神 —— 由呼叫端決定何時查（見 `Providers.tsx`：只對選中的來源按設定的間隔重查）。
 */
import { useState } from "react";
import type { QuotaView } from "../../lib/api";
import { Icon } from "../icons";

/** 用量百分比 → 顏色（cc-switch 的三段式）。 */
export function quotaColor(percent: number | null): string {
  if (percent === null) return "#8e8e93";
  if (percent >= 90) return "#ff453a";
  if (percent >= 70) return "#ff9f0a";
  return "#30d158";
}

/** 數字顯示：大數字不留一堆小數。 */
function fmtNum(v: number | null, unit: string): string {
  if (v === null) return "—";
  const n = Math.abs(v) >= 1000 ? Math.round(v).toLocaleString() : v.toFixed(2);
  return unit ? `${n} ${unit}` : n;
}

export function QuotaFooter(props: {
  q: QuotaView | null;
  loading: boolean;
  onRefresh: () => void;
  onConfigure: () => void;
}) {
  const [open, setOpen] = useState(false);
  const { q } = props;

  // 沒設定、或查詢中還沒有結果 → 只留一個入口（不要讓卡片變吵）
  if (!q) {
    return (
      <div className="mt-2 flex items-center gap-2 border-t border-fg/[0.06] pt-2">
        <button
          className="flex items-center gap-1 text-[11px] text-fg/30 transition-colors hover:text-fg/70"
          onClick={(e) => {
            e.stopPropagation();
            props.onConfigure();
          }}
          title="設定這個來源的用量查詢（餘額／方案額度）"
        >
          <Icon name="chart-bar" size={11} />
          用量查詢
        </button>
        {props.loading && <span className="text-[10px] text-fg/25">查詢中…</span>}
      </div>
    );
  }

  const col = quotaColor(q.percent);
  return (
    <div className="mt-2 border-t border-fg/[0.06] pt-2">
      <div className="flex flex-wrap items-center gap-2">
        {q.ok ? (
          <>
            {q.percent !== null && (
              <span
                className="rounded px-1.5 py-px text-[11px] font-medium tabular-nums"
                style={{ background: `${col}22`, color: col }}
                title="用量百分比（<70% 綠 / 70–89% 橘 / ≥90% 紅）"
              >
                {q.percent < 10 ? q.percent.toFixed(1) : Math.round(q.percent)}%
              </span>
            )}
            <span className="text-[11px] text-fg/60">
              {q.plan_name ? `${q.plan_name} · ` : ""}
              餘額 {fmtNum(q.remaining, q.unit)}
              {q.total !== null && (
                <span className="text-fg/30"> / {fmtNum(q.total, q.unit)}</span>
              )}
            </span>
            {q.plans.length > 0 && (
              <button
                className="rounded bg-fg/[0.06] px-1.5 py-px text-[10px] text-fg/50 hover:text-fg/80"
                onClick={(e) => {
                  e.stopPropagation();
                  setOpen((v) => !v);
                }}
              >
                {q.plans.length} 個方案 {open ? "▴" : "▾"}
              </button>
            )}
          </>
        ) : (
          <span className="min-w-0 flex-1 truncate text-[11px] text-amber-300/80" title={q.message}>
            {q.message || "查不到用量"}
          </span>
        )}
        <span className="flex-1" />
        <button
          className="shrink-0 rounded p-0.5 text-fg/30 transition-colors hover:text-fg/80"
          title="重新查詢"
          onClick={(e) => {
            e.stopPropagation();
            props.onRefresh();
          }}
        >
          <Icon name="refresh" size={11} className={props.loading ? "animate-spin" : ""} />
        </button>
        <button
          className="shrink-0 rounded p-0.5 text-fg/30 transition-colors hover:text-fg/80"
          title="用量查詢設定"
          onClick={(e) => {
            e.stopPropagation();
            props.onConfigure();
          }}
        >
          <Icon name="sliders" size={11} />
        </button>
      </div>

      {open && q.plans.length > 0 && (
        <div className="mt-1.5 space-y-0.5">
          {q.plans.map((p, i) => {
            const pct =
              p.total && p.total > 0
                ? Math.max(0, Math.min(100, ((p.used ?? p.total - (p.remaining ?? 0)) / p.total) * 100))
                : null;
            return (
              <div key={`${p.name}-${i}`} className="flex items-center gap-2 text-[11px]">
                <span className="min-w-0 flex-1 truncate text-fg/50">{p.name || "方案"}</span>
                {pct !== null && (
                  <span className="tabular-nums" style={{ color: quotaColor(pct) }}>
                    {Math.round(pct)}%
                  </span>
                )}
                <span className="tabular-nums text-fg/40">
                  {fmtNum(p.remaining, p.unit)}
                </span>
              </div>
            );
          })}
        </div>
      )}
      {q.extra && (
        <div className="pt-0.5 text-[10px] text-fg/25">{q.extra}</div>
      )}
    </div>
  );
}
