/**
 * API 入口（barrel）。
 *
 * 原本這支檔案 570 行，超過專案的 400 行上限；現在拆成 `api/` 底下三塊再合併 ——
 * 對外仍是同一個 `api` 物件，既有 `api.providerList()` 這種寫法完全不用改。
 */
import { useQuery } from "@tanstack/react-query";
import { coreApi } from "./api/core";
import { panelsApi } from "./api/panels";
import { platformApi } from "./api/platform";
import { DEFAULT_GATEWAY_PORT } from "./apiTypes";

export const api = {
  ...coreApi,
  ...panelsApi,
  ...platformApi,
};

export * from "./apiTypes";

/** 讀設定中的網關係口（緩存 30s，失敗回退預設值）。 */
export function useGatewayPort(): number {
  const q = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: 30000,
  });
  return q.data?.gateway_port ?? DEFAULT_GATEWAY_PORT;
}
