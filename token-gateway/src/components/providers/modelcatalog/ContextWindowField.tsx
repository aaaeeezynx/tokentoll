/**
 * 上下文視窗欄位：常用值下拉 ＋ 自訂輸入。
 *
 * 為什麼不是純下拉：來源匯入／複製會帶任意數值（例如 300000），純下拉找不到
 * 對應選項就只能顯示「—」，一存還會被改掉。所以保留「自訂…」入口。
 *
 * 「不指定」= 存 NULL：Codex 目錄會用後端的 [`DEFAULT_CONTEXT_WINDOW`]，
 * 其他工具（Claude Code／OpenCode）則不寫入上限。
 */
import { useEffect, useState } from "react";
import { PopSelect } from "../../PopSelect";

/**
 * 預設上下文視窗（256K）。**必須與後端 `models::DEFAULT_CONTEXT_WINDOW` 一致**。
 *
 * 由來：舊版目錄生成在模型沒填時不覆寫，於是模板（收編自 cc-switch）寫死的
 * 1,000,000 會漏進每一個沒填的模型，Codex 因此遲不壓縮、把請求堆到上游 400。
 */
export const DEFAULT_CONTEXT_WINDOW = 262144;

/** 常用上下文視窗（token）。256K 為預設，其餘供快速選擇。 */
export const CONTEXT_PRESETS: { value: number; label: string; hint?: string }[] = [
  { value: 32768, label: "32K" },
  { value: 65536, label: "64K" },
  { value: 131072, label: "128K" },
  { value: 200000, label: "200K" },
  { value: 262144, label: "256K", hint: "預設" },
  { value: 524288, label: "512K" },
  { value: 1000000, label: "1M" },
  { value: 2000000, label: "2M" },
];

/** 下拉的兩個哨兵值（不會與真實 token 數衝突）。 */
const UNSET = "__unset";
const CUSTOM = "__custom";

export function ContextWindowField(props: {
  /** 草稿值（字串；空字串 = 不指定）。 */
  value: string;
  onChange: (v: string) => void;
  /** 自訂輸入框的樣式（兩個呼叫端的欄位密度不同）。 */
  inputClassName: string;
}) {
  const v = props.value.trim();
  const matched = CONTEXT_PRESETS.find((p) => String(p.value) === v);
  const isBlank = v === "";
  const [forceCustom, setForceCustom] = useState(false);

  // 外部重置草稿（例如列展開）時，值若回到已知選項或空白就退出自訂模式。
  // 使用者正在輸入自訂值時 matched 為 undefined，不會被這裡踢出去。
  useEffect(() => {
    if (isBlank || matched) setForceCustom(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.value]);

  const showInput = forceCustom || (!matched && !isBlank);
  const selectValue = showInput ? CUSTOM : isBlank ? UNSET : String(matched!.value);

  return (
    <div className="min-w-0 space-y-1">
      <PopSelect
        value={selectValue}
        onChange={(next) => {
          if (next === UNSET) {
            setForceCustom(false);
            props.onChange("");
          } else if (next === CUSTOM) {
            setForceCustom(true);
          } else {
            setForceCustom(false);
            props.onChange(next);
          }
        }}
        options={[
          ...CONTEXT_PRESETS.map((p) => ({
            value: String(p.value),
            label: p.label,
            hint: p.hint,
          })),
          { value: UNSET, label: "不指定", hint: "其他工具不寫上限" },
          { value: CUSTOM, label: "自訂…" },
        ]}
      />
      {showInput && (
        <input
          className={props.inputClassName}
          inputMode="numeric"
          placeholder={`自訂 token 數（預設 ${DEFAULT_CONTEXT_WINDOW}）`}
          value={props.value}
          onChange={(e) => props.onChange(e.target.value.replace(/[^0-9]/g, ""))}
        />
      )}
    </div>
  );
}
