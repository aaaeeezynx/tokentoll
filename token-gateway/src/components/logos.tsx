import type { ComponentType } from "react";
import {
  Antigravity,
  ClaudeCode,
  Cursor as CursorIcon,
  DeepSeek,
  HermesAgent,
  OpenAI,
  OpenCode,
} from "@lobehub/icons";

/** 各本機工具品牌標誌（LobeHub 彩色版）。 */
export type LogoName =
  | "claude"
  | "codex"
  | "opencode"
  | "hermes"
  | "dsh"
  | "cursor"
  | "antigravity"
  | "unknown";

export const APP_META: { id: LogoName; label: string }[] = [
  { id: "claude", label: "Claude Code" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
  // 以下兩項的顯示名必須與後端 `tools::consts::APPS` 一致。
  // 這裡指的是**本機工具**（CLI），不是模型或廠商名稱 —— 所以是
  // 「DeepSeek Harness」而非容易誤解的「DeepSeek」。
  { id: "hermes", label: "Hermes Agent" },
  { id: "dsh", label: "DeepSeek Harness" },
  { id: "cursor", label: "Cursor" },
  { id: "antigravity", label: "Antigravity" },
];

export function appLabel(app: string): string {
  return APP_META.find((m) => m.id === app)?.label ?? app;
}

type BrandIcon = ComponentType<{
  size?: number | string;
  className?: string;
  style?: React.CSSProperties;
}>;
/** 有彩色版用彩色版，否則用 Mono 本體（OpenAI 黑白花、Cursor 原生配色）。 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const pick = (icon: any): BrandIcon => (icon.Color ?? icon) as BrandIcon;
const COLOR_MAP: Record<Exclude<LogoName, "unknown">, BrandIcon> = {
  claude: pick(ClaudeCode),
  codex: pick(OpenAI),
  opencode: pick(OpenCode),
  hermes: pick(HermesAgent),
  dsh: pick(DeepSeek),
  cursor: pick(CursorIcon),
  antigravity: pick(Antigravity),
};

export function Logo({
  name,
  size = 16,
  className,
}: {
  name: string;
  size?: number;
  className?: string;
}) {
  if (!(name in COLOR_MAP)) {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        className={className}
        aria-hidden="true"
      >
        <circle
          cx="12"
          cy="12"
          r="7"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeDasharray="3 2.4"
        />
      </svg>
    );
  }
  const C = COLOR_MAP[name as Exclude<LogoName, "unknown">];
  return <C size={size} className={className} />;
}
