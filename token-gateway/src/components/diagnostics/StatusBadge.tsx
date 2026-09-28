//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

/** 上游狀態碼徽章。 */
export function StatusBadge({ status }: { status: number }) {
  if (!status) {
    return <span className="font-mono text-[11px] text-white/30">—</span>;
  }
  const c = status < 400 ? "#30d158" : status < 500 ? "#ffd60a" : "#ff6961";
  return (
    <span
      className="rounded px-1.5 py-px font-mono text-[11px] font-semibold"
      style={{ background: `${c}22`, color: c }}
    >
      {status}
    </span>
  );
}
