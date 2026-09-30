import type { Dispatch, SetStateAction } from "react";
import { RangePicker, type CustomRange } from "../RangePicker";
import { Segmented } from "../Segmented";
import { SCENARIOS, type Toks } from "./calcMath";

export function ScenarioCard(props: {
  scenario: string;
  onPickScenario: (id: string) => void;
  dateRange: CustomRange | null;
  onPickDates: (c: CustomRange | null) => void;
  toks: Toks;
  setToks: Dispatch<SetStateAction<Toks>>;
  num: (v: string) => number;
}) {
  return (
    // ── 用量場景 ──
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-fg/80">
        用量場景
      </div>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="min-w-0 flex-1">
          <Segmented
            className="w-full"
            value={props.scenario}
            onChange={props.onPickScenario}
            btnClassName="px-2 py-1.5 text-xs font-medium"
            options={SCENARIOS.filter((s) => s.id !== "dates").map((s) => ({
              id: s.id,
              label: s.label,
            }))}
          />
        </div>
        <RangePicker
          presets={[]}
          range="30d"
          custom={props.dateRange}
          onPreset={() => {}}
          onCustom={props.onPickDates}
          btnClassName="px-3 py-1.5 text-xs font-medium"
        />
      </div>
      <div className="grid grid-cols-2 gap-x-4 md:grid-cols-4">
        {(
          [
            ["inTok", "輸入 tokens"],
            ["outTok", "輸出 tokens"],
            ["cr", "快取讀 tokens"],
            ["cw", "快取建 tokens"],
          ] as const
        ).map(([k, label]) => (
          <label key={k} className="mb-1 block text-[11px] text-fg/40">
            {label}
            <input
              className="field mt-1 w-full px-2.5 py-1.5 text-xs"
              value={props.toks[k]}
              onChange={(e) => {
                props.setToks((t) => ({ ...t, [k]: props.num(e.target.value) }));
                props.onPickScenario("custom");
              }}
            />
          </label>
        ))}
      </div>
    </div>
  );
}
