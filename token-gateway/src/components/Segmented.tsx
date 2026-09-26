import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

export interface SegOption {
  id: string;
  label: ReactNode;
}

/** macOS Segmented Control：滑動指示塊＋spring 曲線。 */
export function Segmented(props: {
  options: SegOption[];
  value: string;
  onChange: (id: string) => void;
  className?: string;
  btnClassName?: string;
}) {
  const btns = useRef(new Map<string, HTMLButtonElement>());
  const rootRef = useRef<HTMLDivElement>(null);
  const [thumb, setThumb] = useState({ left: 0, width: 0, show: false });

  const measure = () => {
    const el = btns.current.get(props.value);
    if (el) {
      setThumb({ left: el.offsetLeft, width: el.offsetWidth, show: true });
    } else {
      setThumb((t) => ({ ...t, show: false }));
    }
  };

  useLayoutEffect(measure, [props.value, props.options.length]);
  useLayoutEffect(() => {
    window.addEventListener("resize", measure);
    // 容器寬度變化（側欄收合／字體載入／橫滑）時重對齊指示塊
    const ro =
      typeof ResizeObserver !== "undefined" && rootRef.current
        ? new ResizeObserver(measure)
        : null;
    if (ro && rootRef.current) ro.observe(rootRef.current);
    return () => {
      window.removeEventListener("resize", measure);
      ro?.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.value]);

  return (
    <div ref={rootRef} className={`seg relative ${props.className ?? ""}`}>
      <span
        className="seg-thumb z-0"
        style={{
          width: thumb.width,
          transform: `translateX(${thumb.left}px)`,
          opacity: thumb.show ? 1 : 0,
        }}
      />
      {props.options.map((o) => (
        <button
          key={o.id}
          ref={(el) => {
            if (el) btns.current.set(o.id, el);
            else btns.current.delete(o.id);
          }}
          onClick={() => props.onChange(o.id)}
          data-active={props.value === o.id}
          className={`seg-btn relative z-[1] flex-1 items-center justify-center ${props.btnClassName ?? "px-3 py-1 text-[13px] font-medium"}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
