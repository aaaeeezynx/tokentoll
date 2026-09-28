//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

/** 統計卡。 */
export function Stat(props: {
  label: string;
  value: number | string;
  tone?: "warn" | "ok" | "muted";
  hint?: string;
}) {
  const color =
    props.tone === "warn"
      ? "#ff6961"
      : props.tone === "ok"
        ? "#30d158"
        : "var(--mac-label-2)";
  return (
    <div className="glass px-3.5 py-3">
      <div className="text-[11px] text-white/40">{props.label}</div>
      <div
        className="pt-0.5 font-mono text-[19px] leading-tight font-semibold"
        style={{ color }}
      >
        {props.value}
      </div>
      {props.hint && (
        <div className="pt-1 text-[10px] leading-tight text-white/25">
          {props.hint}
        </div>
      )}
    </div>
  );
}
