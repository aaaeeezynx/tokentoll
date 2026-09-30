//! 由 `components/providers/ProviderForm.tsx` 拆分而來。程式碼語意未改，只搬位置。

import {
  useState,
} from "react";
import {
  type Preset,
  type Provider,
} from "../../../lib/api";
import {
  Icon,
} from "../../icons";
import {
  PresetPicker,
} from "../PresetPicker";
import {
  ProviderForm,
} from "./ProviderForm";

/** 來源設定 sheet（與本機工具切換卡同設計語言）。 */
export function ProviderSheet(props: {
  initial: Provider | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [closing, setClosing] = useState(false);
  /** 新建時先給預設清單（對齊 CC Switch 的「+ → 選預設或自訂」）。 */
  const [picking, setPicking] = useState(props.initial === null);
  const [preset, setPreset] = useState<Preset | null>(null);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 180);
  };
  const newMode = props.initial === null;
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
            className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/[0.1] hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/65">
            <Icon name="server" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-white">
            {props.initial
              ? `編輯「${props.initial.name}」`
              : newMode && picking
                ? "新增來源 · 選預設"
                : "新增來源"}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto overflow-x-clip px-5 py-5">
          {newMode && picking ? (
            <PresetPicker
              onPick={(p) => {
                setPreset(p);
                setPicking(false);
              }}
              onCustom={() => {
                setPreset(null);
                setPicking(false);
              }}
            />
          ) : (
            <>
              {newMode && preset && (
                <div className="mb-3 flex items-center gap-2 rounded-lg bg-white/[0.04] px-2.5 py-2">
                  <Icon name="bolt-fill" size={12} />
                  <span className="min-w-0 flex-1 truncate text-xs text-white/60">
                    預設「{preset.name}」已填入連線資訊，可再改
                  </span>
                  <button
                    className="btn-ghost shrink-0 px-2 py-1 text-[11px]"
                    onClick={() => {
                      setPreset(null);
                      setPicking(true);
                    }}
                  >
                    重選
                  </button>
                </div>
              )}
              <ProviderForm
                key={props.initial?.id ?? preset?.id ?? "new"}
                initial={props.initial}
                preset={preset}
                onDone={props.onSaved}
                onCancel={requestClose}
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
