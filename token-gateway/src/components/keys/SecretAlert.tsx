import { useState } from "react";
import { Icon } from "../icons";
import { copyText } from "./shared";

/** macOS Alert：簽發／輪換成功後顯示完整 Key。 */
export function SecretAlert(props: {
  secret: string;
  reason: "new" | "rotated";
  onClose: () => void;
}) {
  const [closing, setClosing] = useState(false);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 200);
  };
  return (
    <div
      className={`fixed inset-0 z-[90] flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div className={`w-full max-w-md glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}>
        <div className="flex items-start gap-3">
          <span
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px]"
            style={{
              background: "color-mix(in srgb, var(--mac-accent) 18%, transparent)",
              color: "var(--mac-accent)",
            }}
          >
            <Icon name={props.reason === "new" ? "key-fill" : "refresh"} size={20} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[16px] font-semibold tracking-tight text-white">
              {props.reason === "new" ? "Key 已簽發" : "Key 已輪換"}
            </div>
            <p className="mt-1 text-[13px] leading-relaxed text-white/55">
              此 Key 已存於本機，可隨時在列表點「查看」再次顯示與複製。
            </p>
          </div>
          <button
            className="shrink-0 rounded-full bg-white/[0.055] p-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
        </div>
        <div className="mt-4 rounded-xl border border-white/[0.08] bg-black/25 p-3">
          <div className="mb-1.5 text-[11px] font-medium text-white/35">完整 Key</div>
          <code className="block max-h-28 select-all overflow-y-auto break-all font-mono text-[13px] leading-relaxed text-white/90">
            {props.secret}
          </code>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button className="btn-ghost px-4 py-2 text-sm" onClick={requestClose}>
            關閉
          </button>
          <button
            className="btn-primary flex items-center gap-1.5 px-4 py-2 text-sm"
            onClick={() => copyText(props.secret)}
          >
            <Icon name="copy" size={14} />
            複製 Key
          </button>
        </div>
      </div>
    </div>
  );
}
