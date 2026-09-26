import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./icons";
import { Logo } from "./logos";

export interface PopOption {
  value: string;
  label: string;
  hint?: string;
  logo?: string;
}

/**
 * 蘋果風格彈出按鈕（替代原生 select）：
 * 圓角 bezel 按鈕＋浮動圓角面板＋選中勾，portal 渲染不被裁剪。
 */
export function PopSelect(props: {
  value: string;
  options: PopOption[];
  onChange: (v: string) => void;
  className?: string;
  title?: string;
  /** 欄位標題（顯示在按鈕上方小字）。 */
  label?: string;
  /** 長列表按名稱過濾。 */
  searchable?: boolean;
  /** 多選模式：value 為逗號分隔字串，點選不自動關閉。 */
  multi?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ left: 0, top: 0, width: 180, up: false });
  const [query, setQuery] = useState("");
  const btnRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const selectedSet = props.multi
    ? new Set(props.value.split(",").map((s) => s.trim()).filter(Boolean))
    : null;
  const current = props.options.find((o) => o.value === props.value);
  const shown = props.searchable && query.trim()
    ? props.options.filter((o) => {
        const q = query.trim().toLowerCase();
        return (
          o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q)
        );
      })
    : props.options;

  const place = () => {
    const btn = btnRef.current;
    if (!btn) return;
    const r = btn.getBoundingClientRect();
    const estH =
      Math.min(280, props.options.length * 34 + 12) +
      (props.searchable ? 44 : 0);
    const up = r.bottom + estH + 8 > window.innerHeight && r.top - estH - 8 > 0;
    setPos({
      left: Math.max(8, Math.min(r.left, window.innerWidth - Math.max(r.width, 170) - 8)),
      top: up ? r.top - estH - 6 : r.bottom + 6,
      width: Math.max(r.width, 170),
      up,
    });
  };

  useEffect(() => {
    if (!open) return;
    const inside = (t: EventTarget | null) =>
      !!t &&
      (btnRef.current?.contains(t as Node) ||
        panelRef.current?.contains(t as Node));
    // 點面板內（選項）不關，交給選項 onClick 處理
    const onDown = (e: MouseEvent) => {
      if (!inside(e.target)) setOpen(false);
    };
    // 面板自身滾動（長列表）不動；其餘滾動（sheet／主區）跟隨按鈕重定位，
    // 按鈕已離開視口才關閉，避免長表單裡一點滾就收起
    const onScroll = (e: Event) => {
      if (inside(e.target)) return;
      const r = btnRef.current?.getBoundingClientRect();
      if (!r || r.bottom < 0 || r.top > window.innerHeight) {
        setOpen(false);
        return;
      }
      place();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onResize = () => setOpen(false);
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onResize);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", onKey);
    };
  }, [open ]);

  const toggle = () => {
    if (!open) {
      setQuery("");
      place();
    }
    setOpen((o) => !o);
  };

  return (
    <div className={props.className ?? ""}>
      {props.label && (
        <div className="mb-1 text-[11px] font-medium text-white/40">
          {props.label}
        </div>
      )}
      <button
        ref={btnRef}
        type="button"
        title={props.title}
        onClick={toggle}
        className="field flex w-full items-center gap-1.5 px-2.5 py-1.5 text-[13px] text-white/80"
      >
        {props.multi ? (
          <span className="min-w-0 flex-1 truncate text-left">
            {selectedSet && selectedSet.size > 0
              ? props.options
                  .filter((o) => selectedSet.has(o.value))
                  .map((o) => o.label)
                  .join(", ")
              : "—"}
          </span>
        ) : (
          <>
            {current?.logo && <Logo name={current.logo} size={14} className="shrink-0" />}
            <span className="min-w-0 flex-1 truncate text-left">
              {current ? current.label : "—"}
            </span>
          </>
        )}
        <Icon
          name="chevron-down"
          size={12}
          className={`shrink-0 text-white/30 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open &&
        createPortal(
          <div
            ref={panelRef}
            className="glass-panel pop-in z-[100] overflow-y-auto p-1.5"
            style={{
              position: "fixed",
              left: pos.left,
              top: pos.top,
              width: pos.width,
              maxHeight: 280,
            }}
          >
            {props.searchable && (
              <div className="sticky top-0 -mx-1.5 -mt-1.5 bg-[#18181e] px-1.5 pt-1.5 pb-1.5">
                <div className="relative">
                  <Icon
                    name="search"
                    size={12}
                    className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-white/25"
                  />
                  <input
                    autoFocus
                    className="field w-full py-1.5 pr-2 pl-7 text-xs"
                    placeholder="搜尋名稱…"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </div>
              </div>
            )}
            {shown.length === 0 && (
              <p className="px-2 py-3 text-center text-xs text-white/25">
                無匹配
              </p>
            )}
            {shown.map((o) => {
              const active = props.multi
                ? selectedSet?.has(o.value) ?? false
                : o.value === props.value;
              return (
                <button
                  key={o.value}
                  type="button"
                  onClick={() => {
                    if (props.multi) {
                      const next = new Set(selectedSet);
                      if (next.has(o.value)) next.delete(o.value);
                      else next.add(o.value);
                      props.onChange([...next].join(","));
                    } else {
                      props.onChange(o.value);
                      setOpen(false);
                    }
                  }}
                  data-active={active}
                  className={`pop-opt flex w-full items-center gap-2 rounded-lg px-2 py-[7px] text-left text-[13px] transition-colors ${
                    active
                      ? ""
                      : "text-white/70 hover:bg-white/[0.07] hover:text-white"
                  }`}
                >
                  {props.multi ? (
                    <span
                      className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border transition-colors ${
                        active
                          ? "border-(--mac-accent) bg-(--mac-accent)"
                          : "border-white/20 bg-transparent"
                      }`}
                    >
                      {active && <Icon name="check" size={11} className="text-white" />}
                    </span>
                  ) : (
                    <>
                      {o.logo && <Logo name={o.logo} size={14} className="shrink-0" />}
                    </>
                  )}
                  <span className="min-w-0 flex-1 truncate">{o.label}</span>
                  {o.hint && (
                    <span className="max-w-[45%] truncate text-[11px] text-white/25">
                      {o.hint}
                    </span>
                  )}
                  {!props.multi && active && (
                    <Icon name="check" size={13} className="pop-check shrink-0" />
                  )}
                </button>
              );
            })}
          </div>,
          document.body,
        )}
    </div>
  );
}
