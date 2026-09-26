import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

export interface DbStatus {
  path: string;
  schema_version: number;
  provider_count: number;
  key_count: number;
  log_count: number;
}

export interface ProxyStatus {
  running: boolean;
  listen: string;
  port: number;
  started_at: number | null;
}

export interface Provider {
  id: number;
  name: string;
  app_type: string;
  api_format: string;
  base_url: string;
  api_key: string;
  /** bearer（預設）| goog-key（Google x-goog-api-key） */
  auth_scheme: string;
  models: string[];
  priority: number;
  enabled: boolean;
}

export interface ProviderInput {
  name: string;
  app_type: string;
  api_format: string;
  base_url: string;
  api_key: string;
  auth_scheme: string;
  models: string[];
  priority: number;
  enabled: boolean;
}

export interface ToolStatus {
  app: string;
  display: string;
  installed: boolean;
  config_path: string | null;
  current_base_url: string | null;
  current_model: string | null;
  gateway_active: boolean;
  supported_switch: boolean;
}

export interface ToolVersion {
  app: string;
  display: string;
  installed: boolean;
  version: string | null;
  update_argv: string[];
  update_label: string | null;
  up_to_date: boolean | null;
}

export interface ToolLatest {
  app: string;
  latest: string | null;
  up_to_date: boolean | null;
}

export interface SwitchRequest {
  app: string;
  base_url: string;
  api_key: string;
  model: string;
  via_gateway: boolean;
  provider_id: number | null;
  provider_format: string | null;
  reasoning: string | null;
  context_window: number | null;
  gen_catalog: boolean;
  catalog_union: boolean;
  direct_upstream: boolean;
  /** 下拉選中的本地 Key id（僅回填選項；後端以 api_key 為準）。 */
  key_id?: number | null;
  /** Claude Code 模型映射（僅 claude 有效）。 */
  claude_map?: {
    default: string;
    haiku: string;
    sonnet: string;
    opus: string;
    fable: string;
  } | null;
}

export interface SwitchPlan {
  app: string;
  config_path: string;
  supported: boolean;
  will_backup: boolean;
  edits: string[];
  env_setup: string[];
  warnings: string[];
}

export interface SwitchResult {
  config_path: string;
  backup_path: string | null;
  extra_files: string[];
}

export interface AppSettings {
  gateway_port: number;
  auto_start_proxy: boolean;
  accent: string;
}

export interface LocalKey {
  id: number;
  name: string;
  key_prefix: string;
  quota_tokens: number;
  used_tokens: number;
  rate_limit_qpm: number;
  allowed_models: string[];
  allowed_apps: string[];
  provider_id: number | null;
  provider_name: string | null;
  expires_at: number | null;
  enabled: boolean;
  created_at: number;
}

export interface KeyInput {
  name: string;
  provider_id: number | null;
  quota_tokens: number;
  rate_limit_qpm: number;
  allowed_models: string[];
  allowed_apps: string[];
  expires_at: number | null;
}

export interface KeyCreated {
  id: number;
  /** 明文，僅返回一次，UI 必須提示立即複製。 */
  secret: string;
}

export interface LogRow {
  id: number;
  ts: number;
  key_id: number | null;
  app: string;
  provider_id: number | null;
  model_raw: string;
  in_tok: number;
  out_tok: number;
  cache_read: number;
  cache_write: number;
  cost_usd: number;
  latency_ms: number;
  status: number;
  is_stream: boolean;
  /** 'gateway'（即時）| 'import'（歷史回填） */
  source: string;
}

export type UsageRange = "today" | "7d" | "30d" | "90d" | "180d" | "365d";

export interface UsageSummary {
  requests: number;
  ok_requests: number;
  success_rate: number;
  in_tok: number;
  out_tok: number;
  cache_read: number;
  cache_write: number;
  total_tokens: number;
  cache_hit_rate: number;
  cost_usd: number;
}

export interface TrendBucket {
  ts: number;
  requests: number;
  tokens: number;
  cost_usd: number;
}

export interface TrendAppRow {
  ts: number;
  app: string;
  requests: number;
  tokens: number;
  cost_usd: number;
}

export interface HourlyBucket {
  hour: number;
  requests: number;
  tokens: number;
  cost_usd: number;
}

export interface PriceQuote {
  cost_usd: number;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
  source: string;
  mode: string;
  sub_fee_usd: number;
  sub_included_tokens: number;
  sub_used_this_month: number;
  sub_over_tokens: number;
}

export interface SubscriptionFee {
  provider_id: number;
  provider_name: string;
  model_norm: string;
  sub_fee_usd: number;
  sub_included_tokens: number;
  used_this_month: number;
}

export interface SubscriptionFees {
  fees: SubscriptionFee[];
  total_fee: number;
}

export interface QuotaWindow {
  label: string;
  percent: number;
  resets_at: string | null;
}

export interface SubscriptionQuota {
  key: string;
  title: string;
  kind: string;
  status: "ok" | "no_credentials" | "expired" | "error" | "not_subscribed";
  message: string | null;
  windows: QuotaWindow[];
  queried_at: number;
}

export interface ProviderStat {
  provider_id: number | null;
  provider_name: string;
  requests: number;
  tokens: number;
  cost_usd: number;
  success_rate: number;
}

export interface ModelStat {
  model: string;
  requests: number;
  in_tok: number;
  out_tok: number;
  tokens: number;
  cost_usd: number;
}

export interface Pricing {
  model_norm: string;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
  source: string;
}

export interface PricingInput {
  model_norm: string;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
}

export type PricingMode = "usage" | "subscription" | "tou";

export interface ProviderPricing {
  provider_id: number;
  provider_name: string;
  model_norm: string;
  mode: PricingMode;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
  sub_fee_usd: number;
  sub_included_tokens: number;
  source: string;
  used_this_month: number;
  periods: PricingPeriod[];
}

export interface ProviderPricingInput {
  provider_id: number;
  model_norm: string;
  mode: PricingMode;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
  sub_fee_usd: number;
  sub_included_tokens: number;
}

export interface PricingPeriod {
  id: number;
  provider_id: number;
  model_norm: string;
  name: string;
  start: string;
  end: string;
  ord: number;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
}

export interface PricingPeriodInput {
  provider_id: number;
  model_norm: string;
  name: string;
  start: string;
  end: string;
  ord: number;
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
}

export type ReasoningLevel =
  | "unset"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max";

export const REASONING_OPTIONS: { id: ReasoningLevel; label: string }[] = [
  { id: "unset", label: "未設定" },
  { id: "minimal", label: "minimal" },
  { id: "low", label: "low" },
  { id: "medium", label: "medium" },
  { id: "high", label: "high" },
  { id: "xhigh", label: "xhigh" },
  { id: "max", label: "max" },
];

export interface PriceExtractResult {
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
  currency: string;
  confidence: number;
  evidence: string;
  used_model: string;
  source_note: string;
}

export interface CatalogEntry {
  id: string;
}

export interface CatalogCache {
  provider_id: number;
  fetched_at: number;
  models: CatalogEntry[];
}

export interface UsableModel {
  id: number;
  provider_id: number;
  display_name: string;
  actual_model: string;
  context_window: number | null;
  reasoning: string;
  ord: number;
  enabled: boolean;
}

export interface UsableModelInput {
  provider_id: number;
  display_name: string;
  actual_model: string;
  context_window: number | null;
  reasoning: string;
  ord: number;
  enabled: boolean;
}

export interface HistoryScanTool {
  tool: string;
  files: number;
  bytes: number;
  sessions: number;
}

export interface HistoryToolStat {
  tool: string;
  files: number;
  rows: number;
  tokens: number;
  cost_usd: number;
}

export interface HistoryUnpriced {
  model: string;
  tokens: number;
}

export interface HistorySummary {
  files_scanned: number;
  files_imported: number;
  files_unchanged: number;
  rows_added: number;
  dupes_skipped: number;
  empty_skipped: number;
  bad_lines: number;
  in_tok: number;
  out_tok: number;
  cost_usd: number;
  per_tool: HistoryToolStat[];
  unpriced_top: HistoryUnpriced[];
}

/** C 方案：連接埠可在設定中修改；歷史預設 15721 與 cc-switch 衝突，現預設 15722。 */
export const DEFAULT_GATEWAY_PORT = 15722;

export function gatewayUrl(port: number, app: string): string {
  const base = `http://127.0.0.1:${port}`;
  return app === "claude" ? base : `${base}/v1`;
}

// ─────────────────────────────────────────────── 診斷中心（Phase 1）──
// 對應後端 trace.rs / commands.rs 的診斷命令。欄位維持 snake_case，
// 與本檔其餘介面一致（後端未使用 camelCase rename）。

/** 一筆請求追蹤。只在異常時寫入（剝離／4xx／5xx／解析失敗）。 */
export interface TraceRow {
  id: number;
  ts: number;
  /** info | warn */
  level: string;
  app: string;
  model_raw: string;
  /** 入站格式：anthropic | responses | openai | gemini | unknown */
  in_fmt: string;
  /** 渠道協議：openai-chat | openai-responses | mixed | anthropic | gemini */
  target_fmt: string;
  trans_kind: string;
  upstream_status: number;
  latency_ms: number;
  retry_count: number;
  /** 上游拒收而被剝離的欄位名 */
  stripped_fields: string[];
  content_length: number | null;
  content_type: string;
  body_sha256: string;
  /** body 解析失敗時保留的原始位元組 hex 前綴（否則為空） */
  body_hex: string;
  upstream_error: string;
  note: string;
}

/** 某欄位被多少個渠道的上游拒收。 */
export interface StrippedFieldStat {
  field: string;
  providers: number;
}

/** 某渠道學到的上游拒收欄位。 */
export interface ProviderStripped {
  provider_id: number;
  /** 空字串表示渠道已被刪除（孤兒列，仍要看得見） */
  provider_name: string;
  fields: string[];
}

export interface StatusStat {
  status: number;
  count: number;
}

export interface TraceSummary {
  total: number;
  warn_count: number;
  last_24h: number;
  retried: number;
  /** 帶有 body hex 的追蹤數＝body 解析失敗樣本數 */
  with_body_hex: number;
  stripped_fields: StrippedFieldStat[];
  top_status: StatusStat[];
}

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
