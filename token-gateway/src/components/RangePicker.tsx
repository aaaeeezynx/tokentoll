import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Segmented } from "./Segmented";

export interface CustomRange {
  start: number;
  end: number;
}

export const CUSTOM_MAX_DAYS = 365;

function toLocalInput(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

function fmtShort(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** 跨度對應的 bucket 秒數（與後端 range_bucket_span 一致）。 */
export function stepMsFor(range: string, custom: CustomRange | null): number {
  if (range === "today") return 3600 * 1000;
  if (custom) {
    const span = custom.end - custom.start;
    if (span <= 2 * 86400 * 1000) return 3600 * 1000;
    if (span <= 186 * 86400 * 1000) return 86400 * 1000;
    return 7 * 86400 * 1000;
  }
  return 86400 * 1000;
}

export function isHourlyRange(range: string, custom: CustomRange | null): boolean {
  if (range === "today") return true;
  if (custom) return custom.end - custom.start <= 2 * 86400 * 1000;
  return false;
}

/**
 * 預設 presets Segmented ＋ 自訂期間 popover。
 * 自訂啟用時 presets 無選中態，自訂鈕高亮並顯示起止。
 */
export function RangePicker(props: {
  presets: { id: string; label: string }[];
  range: string;
  custom: CustomRange | null;
  onPreset: (id: string) => void;
  onCustom: (c: CustomRange | null) => void;
  btnClassName?: string;
}) {
  const { custom } = props;
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ left: 0, top: 0, width: 280 });
  const btnRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const now = Date.now();
  const [draftStart, setDraftStart] = useState(() =>
    toLocalInput((custom?.start ?? now - 7 * 86400 * 1000)),
  );
  const [draftEnd, setDraftEnd] = useState(() => toLocalInput(custom?.end ?? now));

  useEffect(() => {
    if (custom) {
      setDraftStart(toLocalInput(custom.start));
      setDraftEnd(toLocalInput(custom.end));
    }
  }, [custom, open]);

  const place = () => {
    const btn = btnRef.current;
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    setPos({
      left: Math.max(8, Math.min(r.left, window.innerWidth - 296)),
      top: r.bottom + 6,
      width: 288,
    });
  };

  useEffect(() => {
    if (!open) return;
    const inside = (t: EventTarget | null) =>
      !!t &&
      (btnRef.current?.contains(t as Node) ||
        panelRef.current?.contains(t as Node));
    const onDown = (e: MouseEvent) => {
      if (!inside(e.target)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    // 面板在按鈕下方：跟隨按鈕重定位，按鈕離開視口才關閉
    const onScroll = (e: Event) => {
      if (inside(e.target)) return;
      const r = btnRef.current?.getBoundingClientRect();
      if (!r || r.bottom < 0 || r.top > window.innerHeight) {
        setOpen(false);
        return;
      }
      place();
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [open ]);

  const s = new Date(draftStart).getTime();
  const e = new Date(draftEnd).getTime();
  const err =
    Number.isNaN(s) || Number.isNaN(e)
      ? "時間格式不正確"
      : s >= e
        ? "起始必須早於結束"
        : e - s > CUSTOM_MAX_DAYS * 86400 * 1000
          ? `跨度上限 ${CUSTOM_MAX_DAYS} 天`
          : e > Date.now() + 60 * 1000
            ? "結束不能是未來"
            : "";

  const toggle = () => {
    if (!open) {
      place();
    }
    setOpen((o) => !o);
  };

  const btnCls =
    props.btnClassName ?? "px-3 py-1 text-[13px] font-medium";
  return (
    <>
      {props.presets.length > 0 && (
        <Segmented
          value={custom ? "__custom" : props.range}
          onChange={(v) => {
            if (v !== "__custom") {
              props.onPreset(v);
              props.onCustom(null);
            }
          }}
          btnClassName={btnCls}
          options={props.presets.map((p) => ({ id: p.id, label: p.label }))}
        />
      )}
      <button
        ref={btnRef}
        type="button"
        onClick={toggle}
        className="btn-primary flex items-center gap-1.5 px-4 py-1.5 text-sm"
      >
        {custom ? `自訂 ${fmtShort(custom.start)}–${fmtShort(custom.end)}` : "自訂"}
      </button>
      {open &&
        createPortal(
          <div
            ref={panelRef}
            className="glass-panel pop-in z-[100] p-3.5"
            style={{ position: "fixed", left: pos.left, top: pos.top, width: pos.width }}
          >
            <div className="mb-2 text-[13px] font-semibold text-white/85">
              自訂期間
            </div>
            <label className="mb-1 block text-[11px] text-white/40">
              起始
              <input
                type="datetime-local"
                className="field mt-1 w-full px-2.5 py-1.5 text-[13px]"
                value={draftStart}
                onChange={(e) => setDraftStart(e.target.value)}
              />
            </label>
            <label className="mb-2 block text-[11px] text-white/40">
              結束
              <input
                type="datetime-local"
                className="field mt-1 w-full px-2.5 py-1.5 text-[13px]"
                value={draftEnd}
                onChange={(e) => setDraftEnd(e.target.value)}
              />
            </label>
            {err && <p className="mb-2 text-xs text-red-400">{err}</p>}
            <div className="flex justify-end gap-2">
              <button
                className="btn-ghost px-3.5 py-1.5 text-xs"
                onClick={() => {
                  props.onCustom(null);
                  setOpen(false);
                }}
              >
                清除
              </button>
              <button
                className="btn-primary px-3.5 py-1.5 text-xs disabled:opacity-40"
                disabled={!!err}
                onClick={() => {
                  props.onCustom({ start: s, end: e });
                  setOpen(false);
                }}
              >
                套用
              </button>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}
