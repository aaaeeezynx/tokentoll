/** 平台面 API（外觀與啟動、資料庫備份、視窗行為、Deep Link）。 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppStat,
  CatalogCache,
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
  ProviderPricing,
  ProviderPricingInput,
  ProviderStat,
  DbBackupInfo,
  DbBackupState,
  DbRestoreOutcome,
  WindowBehavior,
  AppearanceState,
  DeeplinkApplied,
  DeeplinkPreview,
  DeeplinkProtocolState,
  SessionApp,
  SessionDeleteOutcome,
  SessionMessage,
  SessionSummary,
  SubscriptionFees,
  SubscriptionQuota,
  SwitchPlan,
  SwitchRequest,
  SwitchResult,
  ToolLatest,
  ToolStatus,
  ToolVersion,
  TrendAppRow,
  TrendBucket,
  UsageRange,
  UsageSummary,
  UsableModel,
  UsableModelInput,
} from "../apiTypes";

export const platformApi = {
  // ── P4.3／P4.4 外觀與啟動 ──
  appearanceState: (): Promise<AppearanceState> => invoke("appearance_state"),
  appearanceSetTheme: (value: string): Promise<string> =>
    invoke("appearance_set_theme", { value }),
  appearanceSetAutostart: (enabled: boolean): Promise<AppearanceState> =>
    invoke("appearance_set_autostart", { enabled }),

  // ── P4.8 資料庫備份管理 ──
  dbBackupState: (): Promise<DbBackupState> => invoke("db_backup_state"),
  dbBackupNow: (name?: string | null): Promise<DbBackupInfo> =>
    invoke("db_backup_now", { name: name ?? null }),
  dbBackupRestore: (name: string): Promise<DbRestoreOutcome> =>
    invoke("db_backup_restore", { name }),
  dbBackupRename: (name: string, newName: string): Promise<DbBackupInfo> =>
    invoke("db_backup_rename", { name, newName }),
  dbBackupDelete: (name: string): Promise<void> =>
    invoke("db_backup_delete", { name }),
  dbBackupSetSchedule: (
    intervalHours?: number,
    retention?: number,
  ): Promise<DbBackupState> =>
    invoke("db_backup_set_schedule", {
      intervalHours: intervalHours ?? null,
      retention: retention ?? null,
    }),

  // ── P4.7 視窗與托盤行為 ──
  windowBehavior: (): Promise<WindowBehavior> => invoke("window_behavior"),
  windowSetCloseAction: (action: string): Promise<WindowBehavior> =>
    invoke("window_set_close_action", { action }),
  /** 進入輕量模式（銷毀視窗、網關繼續跑）。 */
  windowEnterLightweight: (): Promise<void> =>
    invoke("window_enter_lightweight"),
  windowShowMain: (): Promise<void> => invoke("window_show_main"),

  // ── P4.1 Deep Link ──
  /** 啟動時帶進來的連結（取走後就沒有了）。 */
  deeplinkTakePending: (): Promise<string | null> =>
    invoke("deeplink_take_pending"),
  deeplinkPreview: (url: string): Promise<DeeplinkPreview> =>
    invoke("deeplink_preview", { url }),
  deeplinkApply: (url: string): Promise<DeeplinkApplied> =>
    invoke("deeplink_apply", { url }),
  deeplinkProtocolState: (): Promise<DeeplinkProtocolState> =>
    invoke("deeplink_protocol_state"),
  deeplinkRegister: (): Promise<DeeplinkProtocolState> =>
    invoke("deeplink_register"),
  deeplinkUnregister: (): Promise<DeeplinkProtocolState> =>
    invoke("deeplink_unregister"),

  // ── P3.4 會話管理 ──
  sessionsApps: (): Promise<SessionApp[]> => invoke("sessions_apps"),
  /** 掃描會話（apps 空＝全部）。 */
  sessionsScan: (apps?: string[]): Promise<SessionSummary[]> =>
    invoke("sessions_scan", { apps: apps ?? null }),
  sessionsRead: (
    app: string,
    sessionId: string,
    path: string,
  ): Promise<SessionMessage[]> =>
    invoke("sessions_read", { app, sessionId, path }),
  /** 刪除會話（只允許該工具會話目錄底下的檔案）。 */
  sessionsDelete: (
    targets: [string, string][],
  ): Promise<SessionDeleteOutcome> => invoke("sessions_delete", { targets }),
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
