//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import {
  type UsageRange,
} from "../../lib/api";
import {
  type IconName,
} from "../icons";

export const LENSES: { id: LensId; label: string; icon: IconName }[] = [
  { id: "overview", label: "總覽", icon: "chart-bar-fill" },
  { id: "apps", label: "工具", icon: "server" },
  { id: "models", label: "模型", icon: "cpu" },
  { id: "monthly", label: "月", icon: "calendar" },
  { id: "daily", label: "日", icon: "calendar-day" },
  { id: "hourly", label: "時段", icon: "clock" },
  { id: "stats", label: "統計", icon: "chart-line" },
  { id: "channels", label: "來源", icon: "switch" },
];

export const RANGES: { id: UsageRange; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "7d", label: "最近 7 天" },
  { id: "30d", label: "最近 30 天" },
];

export type LensId = "overview" | "apps" | "models" | "monthly" | "daily" | "hourly" | "stats" | "channels";

export type Lens = (typeof LENSES)[number]["id"];

export function fmtInt(n: number): string {
  return Math.round(n).toLocaleString();
}

export function fmtCost(n: number): string {
  return n.toFixed(4);
}

export function fmtPct(n: number): string {
  return `${(n * 100).toFixed(1)}%`;
}

export function fmtBucket(ts: number, hourly: boolean): string {
  const d = new Date(ts);
  if (hourly) {
    return `${d.getHours().toString().padStart(2, "0")}:00`;
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** 各本機工具的主題色（堆疊柱狀圖用）。 */
export const APP_COLORS: Record<string, string> = {
  claude: "#d97757", // Anthropic coral
  codex: "#34d399", // OpenAI mint
  opencode: "#f5f5f7", // 白
  hermes: "#a78bfa", // violet
  dsh: "#4d6bfe", // DeepSeek blue
  cursor: "#94a3b8", // steel gray（呼應 Cursor 黑白品牌）
  antigravity: "#fbbf24",
  unknown: "rgba(255,255,255,0.30)",
};
export const APP_ORDER = ["claude", "codex", "opencode", "hermes", "dsh", "cursor", "antigravity", "unknown"];
export function appColor(app: string): string {
  return APP_COLORS[app] ?? "#8e8e93";
}
