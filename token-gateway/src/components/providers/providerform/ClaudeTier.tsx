//! 由 `components/providers/ProviderForm.tsx` 拆分而來。程式碼語意未改，只搬位置。

import {
  PopSelect,
} from "../../PopSelect";

export type ClaudeTierKey = "default" | "haiku" | "sonnet" | "opus" | "fable";
export const CLAUDE_TIERS: { key: ClaudeTierKey; label: string }[] = [
  { key: "default", label: "預設模型" },
  { key: "haiku", label: "Haiku" },
  { key: "sonnet", label: "Sonnet" },
  { key: "opus", label: "Opus" },
  { key: "fable", label: "Fable" },
];

/** 單一 tier 儲存字串：「」= 留空；「manual:文本」= 手動輸入；其他 = 列表 actual_model。 */
export function parseTierRaw(
  raw: string,
  options: { value: string }[],
): { sel: string; txt: string } {
  if (!raw) return { sel: "", txt: "" };
  if (raw.startsWith("manual:")) return { sel: "manual", txt: raw.slice(7) };
  if (options.some((o) => o.value === raw)) return { sel: raw, txt: "" };
  return { sel: "manual", txt: raw }; // 舊存檔任意字串 → 當手動文本
}

/** Claude 模型映射單檔位：下拉（來源模型＋手動）+ 有效值提示。 */
export function ClaudeTierField(props: {
  label: string;
  options: { value: string; label: string; hint?: string }[];
  raw: string;
  followedBy: string;
  onRaw: (v: string) => void;
}) {
  const { sel, txt } = parseTierRaw(props.raw, props.options);
  const eff =
    sel === "manual"
      ? txt.trim() || props.followedBy
      : sel || props.followedBy;
  return (
    <div>
      <PopSelect
        label={props.label}
        value={sel}
        onChange={(v) =>
          props.onRaw(v === "manual" ? `manual:${txt}` : v)
        }
        options={[
          { value: "", label: "留空：跟隨上方選擇的模型" },
          ...props.options,
          { value: "manual", label: "手動輸入…" },
        ]}
      />
      {sel === "manual" && (
        <input
          className="field mt-1.5 w-full px-3.5 py-2 text-sm"
          placeholder="實際模型 ID（留空=跟隨上方）"
          value={txt}
          onChange={(e) => props.onRaw(`manual:${e.target.value}`)}
        />
      )}
      {eff && (
        <p className="mt-1 font-mono text-xs text-white/40">→ {eff}</p>
      )}
    </div>
  );
}
