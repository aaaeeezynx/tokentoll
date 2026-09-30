/**
 * 外觀（P4.3）：深色／淺色／跟隨系統。
 *
 * 真正的樣式在 `index.css`：`<html data-theme="light|dark">` 決定一整組 CSS 變數。
 * 這裡只負責「決定現在該是哪一個」，包括跟隨系統時監聽系統變化即時切換。
 *
 * 為什麼同時存 localStorage：`settings` 表是唯一真相，但讀它要等 IPC，
 * 開機時先用快取套用，避免「深色閃一下再變淺色」。
 */
export type ThemePref = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const KEY = "tg:theme-pref:v1";

export function loadThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 忽略 */
  }
  return "system";
}

export function saveThemePref(pref: ThemePref) {
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    /* 忽略 */
  }
}

export function systemPrefersDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true;
}

export function resolveTheme(pref: ThemePref): ResolvedTheme {
  if (pref === "light" || pref === "dark") return pref;
  return systemPrefersDark() ? "dark" : "light";
}

/** 套用到 `<html data-theme>`（重複呼叫是安全的）。 */
export function applyTheme(pref: ThemePref): ResolvedTheme {
  const actual = resolveTheme(pref);
  document.documentElement.dataset.theme = actual;
  return actual;
}

/** 跟隨系統時，系統切換要即時反映。回傳取消訂閱函式。 */
export function watchSystemTheme(cb: () => void): () => void {
  const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (!mq) return () => {};
  const handler = () => cb();
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}

export const THEME_LABEL: Record<ThemePref, string> = {
  system: "跟隨系統",
  light: "淺色",
  dark: "深色",
};

export function isThemePref(v: unknown): v is ThemePref {
  return v === "system" || v === "light" || v === "dark";
}
