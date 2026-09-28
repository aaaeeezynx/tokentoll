//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

import { type TraceRow } from "../../lib/api";
import { Icon } from "../icons";
import { StatusBadge } from "./StatusBadge";
import { TraceDetail } from "./TraceDetail";
import { fmtTime } from "./shared";

/** 追蹤列清單（表頭 + 可展開的每一列）。 */
export function TraceList(props: {
  list: TraceRow[];
  expanded: number | null;
  setExpanded: (v: number | null) => void;
}) {
  const { list, expanded, setExpanded } = props;
  return (
    <div className="overflow-hidden rounded-md border border-white/[0.06]">
      {/* 表頭 */}
      <div
        className="hidden items-center gap-2 px-3 py-1.5 text-[10px] text-white/30 sm:flex"
        style={{ background: "rgba(255,255,255,0.03)" }}
      >
        <span className="w-32 shrink-0">時間</span>
        <span className="w-14 shrink-0">應用</span>
        <span className="w-16 shrink-0">狀態</span>
        <span className="min-w-0 flex-1">模型 / 轉換</span>
        <span className="w-28 shrink-0">剝離欄位</span>
        <span className="w-16 shrink-0 text-right">延遲</span>
      </div>
      {list.map((r) => (
        <div key={r.id} className="border-t border-white/[0.05]">
          <button
            className="flex w-full flex-wrap items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-white/[0.03]"
            onClick={() => setExpanded(expanded === r.id ? null : r.id)}
          >
            <span className="w-32 shrink-0 font-mono text-[11px] text-white/45">
              {fmtTime(r.ts).slice(5)}
            </span>
            <span className="w-14 shrink-0 truncate text-[12px] text-white/70">
              {r.app || "—"}
            </span>
            <span className="flex w-16 shrink-0 items-center gap-1">
              <StatusBadge status={r.upstream_status} />
            </span>
            <span className="min-w-0 flex-1 truncate text-[12px] text-white/60">
              <span className="font-mono">{r.model_raw || "—"}</span>
              <span className="px-1.5 text-white/25">·</span>
              <span className="text-[11px] text-white/35">
                {r.in_fmt} → {r.target_fmt}
              </span>
              {r.level === "warn" && (
                <span
                  className="ml-2 rounded px-1.5 py-px text-[10px]"
                  style={{
                    background: "rgba(255,69,58,0.14)",
                    color: "#ff6961",
                  }}
                >
                  異常
                </span>
              )}
            </span>
            <span className="w-28 shrink-0 truncate font-mono text-[11px] text-white/45">
              {r.stripped_fields.length > 0
                ? r.stripped_fields.join(", ")
                : r.retry_count > 0
                  ? `重試 ${r.retry_count} 次`
                  : "—"}
            </span>
            <span className="w-16 shrink-0 text-right font-mono text-[11px] text-white/40">
              {r.latency_ms}ms
            </span>
            <Icon
              name={expanded === r.id ? "chevron-down" : "chevron-right"}
              size={12}
              className="shrink-0 text-white/25"
            />
          </button>
          {expanded === r.id && <TraceDetail row={r} />}
        </div>
      ))}
      {list.length >= 300 && (
        <div className="border-t border-white/[0.05] px-3 py-1.5 text-[10px] text-white/25">
          僅顯示最近 300 筆。清空或調整範圍以查看更新記錄。
        </div>
      )}
    </div>
  );
}
