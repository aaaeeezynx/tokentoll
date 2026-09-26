/** 外觀 Accent（對應 macOS 設定→外觀）。寫 CSS 變量即時生效，localStorage 持久化。 */
/* deep 版白字對比度（bg #141417 實測）：藍 4.78／綠 5.48／紫 5.77／粉 4.73 */
export const ACCENTS = [
  {
    id: "blue",
    label: "藍色",
    value: "#0a84ff",
    deep: "#0070e0",
    hover: "#3395ff",
    soft: "rgba(10,132,255,0.16)",
    shadow: "rgba(10,132,255,0.25)",
  },
  {
    id: "green",
    label: "綠色",
    value: "#30d158",
    deep: "#15793b",
    hover: "#46e06b",
    soft: "rgba(48,209,88,0.16)",
    shadow: "rgba(48,209,88,0.25)",
  },
  {
    id: "purple",
    label: "紫色",
    value: "#a78bfa",
    deep: "#6a4fd0",
    hover: "#bda6ff",
    soft: "rgba(167,139,250,0.16)",
    shadow: "rgba(167,139,250,0.25)",
  },
  {
    id: "pink",
    label: "粉色",
    value: "#f472b6",
    deep: "#c24385",
    hover: "#ff8cc6",
    soft: "rgba(244,114,182,0.16)",
    shadow: "rgba(244,114,182,0.25)",
  },
] as const;

export type AccentId = (typeof ACCENTS)[number]["id"];

const KEY = "tg:accent:v1";

export function loadAccent(): AccentId {
  try {
    const v = localStorage.getItem(KEY);
    if (ACCENTS.some((a) => a.id === v)) return v as AccentId;
  } catch {
    /* 忽略 */
  }
  return "blue";
}

/** 僅套用 CSS 變量（不寫任何持久化；啟動路徑用這個，永不覆蓋已存值）。 */
export function applyAccentVars(id: string) {
  const a = ACCENTS.find((x) => x.id === id) ?? ACCENTS[0];
  const r = document.documentElement.style;
  r.setProperty("--mac-accent", a.value);
  r.setProperty("--mac-accent-deep", a.deep);
  r.setProperty("--mac-accent-hover", a.hover);
  r.setProperty("--mac-accent-soft", a.soft);
  r.setProperty("--mac-accent-shadow", a.shadow);
}

/** localStorage 只做首幀快取（WebView 刷盤非同步，不可做真相源）。 */
export function saveAccentLocal(id: string) {
  try {
    localStorage.setItem(KEY, id);
  } catch {
    /* 忽略 */
  }
}

export function isAccentId(v: unknown): v is AccentId {
  return ACCENTS.some((a) => a.id === v);
}
