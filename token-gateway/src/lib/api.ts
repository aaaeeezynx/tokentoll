import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_GATEWAY_PORT } from "./apiTypes";
import type {
  AppSettings,
  AppStat,
  CatalogCache,
  CheckResult,
  DbStatus,
  HistoryScanTool,
  HistorySummary,
  HourlyBucket,
  ImportReport,
  KeyCreated,
  KeyInput,
  LocalKey,
  LogRow,
  McpImportReport,
  McpInput,
  McpPreset,
  McpServer,
  McpSyncOutcome,
  ModelStat,
  PriceExtractResult,
  PriceQuote,
  Pricing,
  PricingInput,
  PricingPeriod,
  PricingPeriodInput,
  Preset,
  PromptApp,
  PromptBackfill,
  PromptInput,
  PromptPanelState,
  PromptPreset,
  Provider,
  ProviderInput,
  ProviderPricing,
  ProviderPricingInput,
  ProviderStat,
  ProviderStripped,
  ProxyStatus,
  QueueModel,
  QueueRow,
  QuotaView,
  SubscriptionFees,
  SubscriptionQuota,
  SourceHealth,
  SwitchPlan,
  SwitchRequest,
  SwitchResult,
  ToolLatest,
  ToolStatus,
  ToolVersion,
  TraceRow,
  TraceSummary,
  TrendAppRow,
  TrendBucket,
  UsageRange,
  UsageQueryConfig,
  UsageSummary,
  UsageTemplate,
  UsableModel,
  UsableModelInput,
} from "./apiTypes";

// 型別／常數已拆至 apiTypes.ts（含 apiTypesPricing / apiTypesDiagnostics），
// 於此 re-export，讓既有 `from "../lib/api"` 的 import 全部維持可用。
export * from "./apiTypes";

export const api = {
  dbStatus: (): Promise<DbStatus> => invoke("db_status"),
  traceSummary: (): Promise<TraceSummary> => invoke("trace_summary"),
  traceList: (limit?: number): Promise<TraceRow[]> =>
    invoke("trace_list", { limit: limit ?? null }),
  traceProblems: (limit?: number): Promise<TraceRow[]> =>
    invoke("trace_problems", { limit: limit ?? null }),
  traceClear: (): Promise<number> => invoke("trace_clear"),
  providerStrippedList: (providerId: number): Promise<string[]> =>
    invoke("provider_stripped_list", { providerId }),
  providerStrippedAll: (): Promise<ProviderStripped[]> =>
    invoke("provider_stripped_all"),
  providerStrippedClear: (providerId: number): Promise<number> =>
    invoke("provider_stripped_clear", { providerId }),
  sourceHealth: (): Promise<SourceHealth[]> => invoke("source_health"),
  proxyStatus: (): Promise<ProxyStatus> => invoke("proxy_status"),
  proxyCheckPort: (port: number): Promise<void> =>
    invoke("proxy_check_port", { port }),
  proxyStart: (port: number): Promise<ProxyStatus> =>
    invoke("proxy_start", { port }),
  proxyStop: (): Promise<void> => invoke("proxy_stop"),
  settingsGet: (): Promise<AppSettings> => invoke("settings_get"),
  settingsSet: (gateway_port: number, auto_start_proxy: boolean): Promise<AppSettings> =>
    invoke("settings_set", { gatewayPort: gateway_port, autoStartProxy: auto_start_proxy }),
  settingsSetAccent: (accent: string): Promise<string> =>
    invoke("settings_set_accent", { accent }),
  appDataDir: (): Promise<string> => invoke("app_data_dir"),
  providersList: (): Promise<Provider[]> => invoke("providers_list"),
  providerAdd: (input: ProviderInput): Promise<Provider> =>
    invoke("provider_add", { input }),
  providerUpdate: (id: number, input: ProviderInput): Promise<Provider> =>
    invoke("provider_update", { id, input }),
  providerDelete: (id: number): Promise<void> =>
    invoke("provider_delete", { id }),
  providerReorder: (ids: number[]): Promise<void> =>
    invoke("provider_reorder", { ids }),
  /** 複製來源（連模型、每模型價格、訂閱時段一起），回傳新來源的 id。 */
  providerDuplicate: (id: number): Promise<number> =>
    invoke("provider_duplicate", { id }),
  presetsList: (): Promise<Preset[]> => invoke("presets_list"),
  /** 匯出全部來源成 JSON 字串；預設不含金鑰。 */
  providersExport: (includeKeys: boolean): Promise<string> =>
    invoke("providers_export", { includeKeys }),
  /** 匯出到檔案（後端寫檔），回傳實際寫入的完整路徑。 */
  providersExportFile: (includeKeys: boolean): Promise<string> =>
    invoke("providers_export_file", { includeKeys }),
  providersImport: (json: string): Promise<ImportReport> =>
    invoke("providers_import", { json }),
  /** 從檔案路徑匯入（搭配匯出回傳的路徑）。 */
  providersImportFile: (path: string): Promise<ImportReport> =>
    invoke("providers_import_file", { path }),
  /** 來源連線檢查：只測位址可達性，不送模型請求、不帶金鑰。 */
  providerCheck: (baseUrl: string): Promise<CheckResult> =>
    invoke("provider_check", { baseUrl }),
  /** 故障轉移佇列：可挑的模型清單（含「有幾個來源」）。 */
  failoverModels: (): Promise<QueueModel[]> => invoke("failover_models"),
  /** 故障轉移佇列：某個模型的候選來源順序（與請求路徑同一個查詢）。 */
  failoverQueue: (model: string): Promise<QueueRow[]> =>
    invoke("failover_queue", { model }),
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
  // ── P3.1 MCP 管理 ──
  mcpList: (): Promise<McpServer[]> => invoke("mcp_list"),
  mcpPresets: (): Promise<McpPreset[]> => invoke("mcp_presets"),
  mcpUpsert: (input: McpInput): Promise<McpServer> =>
    invoke("mcp_upsert", { input }),
  mcpDelete: (id: number): Promise<McpSyncOutcome[]> =>
    invoke("mcp_delete", { id }),
  /** 設定某個伺服器在某個工具上的啟用（改完立刻同步設定檔）。 */
  mcpSetBinding: (
    id: number,
    target: string,
    enabled: boolean,
  ): Promise<McpSyncOutcome[]> =>
    invoke("mcp_set_binding", { id, target, enabled }),
  /** 一鍵把某個工具的所有伺服器開或關。 */
  mcpSetAppAll: (target: string, enabled: boolean): Promise<McpSyncOutcome[]> =>
    invoke("mcp_set_app_all", { target, enabled }),
  mcpSync: (): Promise<McpSyncOutcome[]> => invoke("mcp_sync"),
  mcpImport: (): Promise<McpImportReport> => invoke("mcp_import"),
  // ── P3.2 提示詞預設集 ──
  promptApps: (): Promise<PromptApp[]> => invoke("prompt_apps"),
  /** 面板狀態（順便做首次啟動匯入）。 */
  promptState: (app: string): Promise<PromptPanelState> =>
    invoke("prompt_state", { app }),
  promptList: (app: string): Promise<PromptPreset[]> =>
    invoke("prompt_list", { app }),
  promptSave: (input: PromptInput): Promise<PromptPreset> =>
    invoke("prompt_save", { input }),
  /** 啟用（切換前會先把檔案內容回填到舊的預設集）。 */
  promptActivate: (id: number): Promise<PromptBackfill> =>
    invoke("prompt_activate", { id }),
  promptDeactivate: (target: string): Promise<void> =>
    invoke("prompt_deactivate", { target }),
  promptDelete: (id: number): Promise<void> => invoke("prompt_delete", { id }),
  promptSync: (target: string): Promise<PromptBackfill> =>
    invoke("prompt_sync", { target }),
  /** 讀目前檔案內容（唯讀）。 */
  promptLive: (target: string): Promise<string> =>
    invoke("prompt_live", { target }),
  toolsDetect: (port: number): Promise<ToolStatus[]> =>
    invoke("tools_detect", { port }),
  toolVersions: (): Promise<ToolVersion[]> => invoke("tool_versions"),
  toolLatest: (): Promise<ToolLatest[]> => invoke("tool_latest"),
  toolUpdate: (app: string): Promise<string> => invoke("tool_update", { app }),
  switchPlan: (req: SwitchRequest, port: number): Promise<SwitchPlan> =>
    invoke("switch_plan", { req, port }),
  switchApply: (req: SwitchRequest, port: number): Promise<SwitchResult> =>
    invoke("switch_apply", { req, port }),
  /**
   * **只**還原接管前備份（逐字寫回，不加工）。
   *
   * 2026-09-28 起 UI 不再呼叫這支 —— 開關的 OFF 走 `switchOff`（還原＋切回原生
   * 來源）。留著是當作安全網／救援入口：需要「完全照備份還原、不要任何加工」時，
   * 可以從命令層直接叫（`監控`→ 見 `docs/TESTING.md` 的還原章節）。
   */
  switchRestore: (appName: string, port: number): Promise<string> =>
    invoke("switch_restore", { appName, port }),
  /**
   * 關閉網關：還原接管前備份，**再**把來源換回該工具的第一方
   * （Codex→OpenAI/GPT、Claude Code→Claude、OpenCode→自己的登入）。
   *
   * 與 `switchRestore` 的差別：那支只忠實還原備份，所以會停在備份當時的
   * 第三方來源（可能就是兩天前的舊快照）。開關的 OFF 走這支。
   */
  switchOff: (appName: string, port: number): Promise<string> =>
    invoke("switch_off", { appName, port }),
  codexDoctor: (port: number): Promise<string[]> =>
    invoke("codex_doctor", { port }),
  keysList: (): Promise<LocalKey[]> => invoke("keys_list"),
  keyCreate: (input: KeyInput): Promise<KeyCreated> =>
    invoke("key_create", { input }),
  keyUpdate: (id: number, input: KeyInput): Promise<void> =>
    invoke("key_update", { id, input }),
  keySetEnabled: (id: number, enabled: boolean): Promise<void> =>
    invoke("key_set_enabled", { id, enabled }),
  keyDelete: (id: number): Promise<void> => invoke("key_delete", { id }),
  keyRotate: (id: number): Promise<KeyCreated> => invoke("key_rotate", { id }),
  keyReveal: (id: number): Promise<string> => invoke("key_reveal", { id }),
  recentLogs: (limit: number): Promise<LogRow[]> =>
    invoke("recent_logs", { limit }),
  historyScan: (): Promise<HistoryScanTool[]> => invoke("history_scan"),
  historyImport: (tools: string[]): Promise<HistorySummary> =>
    invoke("history_import", { tools }),
  usageSummary: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<UsageSummary> =>
    invoke("usage_summary", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageTrend: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<TrendBucket[]> =>
    invoke("usage_trend", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageTrendByApp: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<TrendAppRow[]> =>
    invoke("usage_trend_by_app", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageByProvider: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<ProviderStat[]> =>
    invoke("usage_by_provider", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageByModel: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<ModelStat[]> =>
    invoke("usage_by_model", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  /** 分本機工具統計（「工具」視角）。 */
  usageByApp: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<AppStat[]> =>
    invoke("usage_by_app", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageHourly: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<HourlyBucket[]> =>
    invoke("usage_hourly", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  usageExportCsv: (
    range: UsageRange,
    app?: string | null,
    providerId?: number | null,
    model?: string | null,
    startTs?: number | null,
    endTs?: number | null,
  ): Promise<string> =>
    invoke("usage_export_csv", {
      range,
      app,
      providerId,
      model,
      startTs: startTs ?? null,
      endTs: endTs ?? null,
    }),
  priceQuote: (
    providerId: number | null,
    modelNorm: string,
    inTok: number,
    outTok: number,
    cacheRead: number,
    cacheWrite: number,
  ): Promise<PriceQuote> =>
    invoke("price_quote", {
      providerId,
      modelNorm,
      inTok,
      outTok,
      cacheRead,
      cacheWrite,
    }),
  subscriptionFees: (): Promise<SubscriptionFees> =>
    invoke("subscription_fees"),
  quotaQueryAll: (): Promise<SubscriptionQuota[]> => invoke("quota_query_all"),
  pricingList: (): Promise<Pricing[]> => invoke("pricing_list"),
  pricingUpsert: (input: PricingInput): Promise<void> =>
    invoke("pricing_upsert", { input }),
  pricingDelete: (modelNorm: string): Promise<void> =>
    invoke("pricing_delete", { modelNorm }),
  pricingClear: (): Promise<void> => invoke("pricing_clear"),
  ppList: (providerId?: number | null): Promise<ProviderPricing[]> =>
    invoke("pp_list", { providerId: providerId ?? null }),
  ppUpsert: (input: ProviderPricingInput): Promise<void> =>
    invoke("pp_upsert", { input }),
  ppDelete: (providerId: number, modelNorm: string): Promise<void> =>
    invoke("pp_delete", { providerId, modelNorm }),
  ppPeriods: (providerId: number, modelNorm: string): Promise<PricingPeriod[]> =>
    invoke("pp_periods", { providerId, modelNorm }),
  ppPeriodAdd: (input: PricingPeriodInput): Promise<number> =>
    invoke("pp_period_add", { input }),
  ppPeriodUpdate: (id: number, input: PricingPeriodInput): Promise<void> =>
    invoke("pp_period_update", { id, input }),
  ppPeriodDelete: (id: number): Promise<void> =>
    invoke("pp_period_delete", { id }),
  priceExtract: (
    providerId: number,
    model: string,
    opts: { url?: string | null; pastedText?: string | null; imageBase64?: string | null; targetHint?: string | null },
  ): Promise<PriceExtractResult> =>
    invoke("price_extract", {
      providerId,
      model,
      url: opts.url ?? null,
      pastedText: opts.pastedText ?? null,
      imageBase64: opts.imageBase64 ?? null,
      targetHint: opts.targetHint ?? null,
    }),
  catalogFetch: (providerId: number, apiKey?: string, baseUrl?: string): Promise<CatalogCache> =>
    invoke("catalog_fetch", { providerId, apiKey: apiKey ?? null, baseUrl: baseUrl ?? null }),
  catalogGet: (providerId: number): Promise<CatalogCache> =>
    invoke("catalog_get", { providerId }),
  modelsList: (providerId: number): Promise<UsableModel[]> =>
    invoke("models_list", { providerId }),
  modelCounts: (): Promise<[number, number][]> => invoke("model_counts"),
  modelAdd: (input: UsableModelInput): Promise<number> =>
    invoke("model_add", { input }),
  modelUpdate: (id: number, input: UsableModelInput): Promise<void> =>
    invoke("model_update", { id, input }),
  modelDelete: (id: number): Promise<void> => invoke("model_delete", { id }),
};

/** 讀設定中的網關係口（緩存 30s，失敗回退預設值）。 */
export function useGatewayPort(): number {
  const q = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: 30000,
  });
  return q.data?.gateway_port ?? DEFAULT_GATEWAY_PORT;
}
