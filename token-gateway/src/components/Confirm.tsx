import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./icons";

/**
 * macOS Alert 風格確認框（替代原生 window.confirm）。
 * 置頂渲染（z 高於 sheet 與下拉面板），Esc／點空白關閉。
 */
export function ConfirmDialog(props: {
  title: string;
  message?: string;
  confirmLabel?: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [closing, setClosing] = useState(false);
  const close = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onCancel(), 200);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [closing]);

  return createPortal(
    <div
      className={`fixed inset-0 z-[110] flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div
        className={`w-full max-w-sm glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}
      >
        <div className="flex items-start gap-3">
          <span
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px]"
            style={{ background: "rgba(255,69,58,0.14)", color: "#ff6961" }}
          >
            <Icon name="alert" size={18} />
          </span>
          <div className="min-w-0">
            <div className="text-[15px] font-semibold tracking-tight text-fg">
              {props.title}
            </div>
            {props.message && (
              <p className="mt-1 text-[13px] leading-relaxed break-words text-fg/60">
                {props.message}
              </p>
            )}
          </div>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            className="btn-ghost px-4 py-1.5 text-sm"
            onClick={close}
          >
            取消
          </button>
          <button
            className="btn-danger px-4 py-1.5 text-sm"
            onClick={() => {
              if (closing) return;
              props.onConfirm();
            }}
          >
            {props.confirmLabel ?? "刪除"}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** 呼叫端一行接入：const { dialog, ask } = useConfirm(); …ask("標題", () => doIt(), { message }) */
export function useConfirm() {
  const [req, setReq] = useState<null | {
    title: string;
    message?: string;
    confirmLabel?: string;
    action: () => void;
  }>(null);
  const dialog = req ? (
    <ConfirmDialog
      title={req.title}
      message={req.message}
      confirmLabel={req.confirmLabel}
      onCancel={() => setReq(null)}
      onConfirm={() => {
        const a = req.action;
        setReq(null);
        a();
      }}
    />
  ) : null;
  const ask = (
    title: string,
    action: () => void,
    opts?: { message?: string; confirmLabel?: string },
  ) => setReq({ title, action, ...opts });
  return { dialog, ask };
}
