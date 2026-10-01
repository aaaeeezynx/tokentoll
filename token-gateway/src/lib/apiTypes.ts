/**
 * 全站型別（barrel）。
 *
 * 原本這支檔案 767 行，超過專案的 400 行上限；現在依用途拆到 `types/` 底下，
 * 這裡只負責轉出 —— 既有 `import { X } from "../lib/apiTypes"` 完全不用改。
 */
export * from "./types/core";
export * from "./types/extensions";
export * from "./types/platform";
