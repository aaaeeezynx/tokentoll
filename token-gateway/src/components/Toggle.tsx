/** 蘋果樣式開關（macOS/iOS Switch）。旋鈕只用 left+transform 定位，避免與 utilities 疊加。 */
export function Toggle(props: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  title?: string;
  size?: "md" | "sm";
}) {
  const sm = props.size === "sm";
  // 軌道 - 旋鈕 - 邊距(2px) = 行程
  const travel = sm ? 14 : 18;
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.checked}
      title={props.title}
      disabled={props.disabled}
      onClick={(e) => {
        e.stopPropagation();
        if (!props.disabled) props.onChange(!props.checked);
      }}
      className={`relative shrink-0 rounded-full transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-40 ${
        sm ? "h-[20px] w-[34px]" : "h-[24px] w-[42px]"
      } ${props.checked ? "bg-[#34c759]" : "bg-white/15"}`}
      style={
        props.checked
          ? { boxShadow: "0 0 8px rgba(52,199,89,0.35)" }
          : undefined
      }
    >
      <span
        className={`absolute rounded-full bg-white transition-transform duration-200 ${
          sm ? "h-[16px] w-[16px]" : "h-[20px] w-[20px]"
        }`}
        style={{
          top: 2,
          left: 2,
          transform: `translateX(${props.checked ? travel : 0}px)`,
          boxShadow: "0 1px 4px rgba(0,0,0,0.4)",
        }}
      />
    </button>
  );
}
