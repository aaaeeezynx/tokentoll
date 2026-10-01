/** 擴充面板 API（用量查詢）。 */
import { invoke } from "@tauri-apps/api/core";
import type {
  QuotaView,
  UsageQueryConfig,
  UsageTemplate,
} from "../apiTypes";

export const panelsApi = {
  // ── P2.1 用量查詢 ──
  usageQueryGet: (providerId: number): Promise<UsageQueryConfig> =>
    invoke("usage_query_get", { providerId }),
  usageQuerySet: (config: UsageQueryConfig): Promise<UsageQueryConfig> =>
    invoke("usage_query_set", { config }),
  usageQueryClear: (providerId: number): Promise<void> =>
    invoke("usage_query_clear", { providerId }),
  usageQueryTemplates: (): Promise<UsageTemplate[]> =>
    invoke("usage_query_templates"),
  usageQueryApplyTemplate: (
    providerId: number,
    template: string,
  ): Promise<UsageQueryConfig> =>
    invoke("usage_query_apply_template", { providerId, template }),
  /** 真的打一次查詢（面板的「測試」與卡片的「重新查詢」共用）。 */
  usageQueryRun: (providerId: number): Promise<QuotaView> =>
    invoke("usage_query_run", { providerId }),
  /** 所有已啟用設定的來源各查一次（來源頁載入時）。 */
  usageQueryRunAll: (): Promise<QuotaView[]> => invoke("usage_query_run_all"),
};
