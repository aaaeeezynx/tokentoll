import { useState } from "react";
import { type LocalKey, type Provider } from "../../lib/api";
import { Icon } from "../icons";
import { KeyForm } from "./KeyForm";

export function KeySheet(props: {
  initial: LocalKey | null;
  providers: Provider[];
  onClose: () => void;
  onSaved: () => void;
  onCreatedSecret: (s: string) => void;
}) {
  const [closing, setClosing] = useState(false);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 180);
  };
  return (
    <div
      className={`fixed inset-0 z-[80] flex justify-center overflow-y-auto p-4 glass-backdrop transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div
        className={`my-auto flex max-h-[min(88vh,720px)] w-full max-w-lg flex-col overflow-hidden glass-panel ${closing ? "sheet-out" : "pop-in"}`}
      >
        <div className="flex shrink-0 items-center gap-3 border-b border-white/[0.06] bg-[rgba(24,24,30,0.98)] px-5 py-4">
          <button
            className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/65">
            <Icon name="key" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-white">
            {props.initial ? `編輯「${props.initial.name}」` : "簽發 Key"}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto overflow-x-clip px-5 py-5">
          <KeyForm
            key={props.initial?.id ?? "new"}
            initial={props.initial}
            providers={props.providers}
            onDone={() => {
              props.onSaved();
              requestClose();
            }}
            onCancel={requestClose}
            onCreatedSecret={props.onCreatedSecret}
          />
        </div>
      </div>
    </div>
  );
}
