import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_GATEWAY_PORT } from "./apiTypes";
import type {
  AppSettings,
  AppStat,
  CatalogCache,
  DbStatus,
  HistoryScanTool,
  HistorySummary,
  HourlyBucket,
  KeyCreated,
  KeyInput,
  LocalKey,
  LogRow,
  ModelStat,
  PriceExtractResult,
  PriceQuote,
  Pricing,
  PricingInput,
  PricingPeriod,
  PricingPeriodInput,
  Provider,
  ProviderInput,
  ProviderPricing,
  ProviderPricingInput,
  ProviderStat,
  ProviderStripped,
  ProxyStatus,
  SubscriptionFees,
  SubscriptionQuota,
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
  UsageSummary,
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
  toolsDetect: (port: number): Promise<ToolStatus[]> =>
    invoke("tools_detect", { port }),
  toolVersions: (): Promise<ToolVersion[]> => invoke("tool_versions"),
  toolLatest: (): Promise<ToolLatest[]> => invoke("tool_latest"),
  toolUpdate: (app: string): Promise<string> => invoke("tool_update", { app }),
  switchPlan: (req: SwitchRequest, port: number): Promise<SwitchPlan> =>
    invoke("switch_plan", { req, port }),
  switchApply: (req: SwitchRequest, port: number): Promise<SwitchResult> =>
    invoke("switch_apply", { req, port }),
  switchRestore: (appName: string, port: number): Promise<string> =>
    invoke("switch_restore", { appName, port }),
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
