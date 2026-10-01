/** 核心 API（狀態、來源、模型、金鑰、用量、診斷、設定、匯出）。 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  CheckResult,
  DbStatus,
  ImportReport,
  Preset,
  Provider,
  ProviderInput,
  ProviderStripped,
  ProxyStatus,
  QueueModel,
  QueueRow,
  SourceHealth,
  TraceRow,
  TraceSummary,
} from "../apiTypes";

export const coreApi = {
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
};
