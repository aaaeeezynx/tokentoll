//! 由 `Providers.tsx` 拆分而來（原檔 2,845 行）。程式碼語意未改，只搬位置。

import {
  type ProviderInput,
  type SwitchRequest,
} from "../../lib/api";

export const APP_TYPES = ["universal", "claude", "codex", "opencode", "hermes", "dsh"];
export const API_FORMATS = ["openai-chat", "openai-responses", "mixed", "anthropic", "gemini"];
export const AUTH_SCHEMES = [
  { id: "bearer", label: "Bearer（預設）" },
  { id: "goog-key", label: "Google API Key" },
  { id: "anthropic", label: "Anthropic API Key" },
];

export const EMPTY_INPUT: ProviderInput = {
  name: "",
  app_type: "universal",
  api_format: "openai-chat",
  base_url: "",
  api_key: "",
  auth_scheme: "bearer",
  models: [],
  priority: 0,
  enabled: true,
};

export function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

/** 上次成功切換的參數（行開關一鍵重開用；存本機 localStorage）。 */
export const LAST_KEY = (app: string) => `tg:lastSwitch:v1:${app}`;
export function saveLastSwitch(app: string, req: SwitchRequest) {
  try {
    localStorage.setItem(
      LAST_KEY(app),
      JSON.stringify({ ...req, via_gateway: true, base_url: "" }),
    );
  } catch {
    /* 忽略配額錯誤 */
  }
}
export function loadLastSwitch(app: string): SwitchRequest | null {
  try {
    const raw = localStorage.getItem(LAST_KEY(app));
    if (!raw) return null;
    const r = JSON.parse(raw) as SwitchRequest;
    if (!r || r.app !== app || !r.model) return null;
    return r;
  } catch {
    return null;
  }
}
