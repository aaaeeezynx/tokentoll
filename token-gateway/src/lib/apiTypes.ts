// 型別／常數宣告：核心資料模型（自 api.ts 原樣搬移，未改動任何內容）。
// 其餘型別分置 apiTypesPricing.ts / apiTypesDiagnostics.ts，於此檔尾 re-export。

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

/**
 * 來源預設（對齊 CC Switch 的 provider presets）。
 *
 * 只帶**連線資訊**，刻意不含模型清單：模型名變動快，一律用表單上的
 * 「取得模型清單」對上游現場抓（`catalog_fetch`）。
 */
export interface Preset {
  id: string;
  name: string;
  base_url: string;
  api_format: string;
  auth_scheme: string;
  app_type: string;
  /** 空字串＝沒有特別提醒 */
  note: string;
}

/** 來源匯入的結果回報（skip 語意：已存在的 `(name, base_url)` 不覆蓋）。 */
export interface ImportReport {
  added: number;
  skipped: number;
  models: number;
  pricing: number;
  periods: number;
  names: string[];
  /** 匯入檔是否帶金鑰（要提醒使用者） */
  includes_keys: boolean;
}

/**
 * 來源連線檢查的結果（P1.5，對標 CC Switch 的 connectivity check）。
 *
 * `reachable` 是**傳輸層**的判斷：401／403／404 都算可達 ——
 * 它們證明主機活著，只是沒有金鑰或那個路徑沒有端點。
 * 「檢查過了」不等於「請求一定成功」（金鑰與模型名是另一回事）。
 */
export interface CheckResult {
  url: string;
  reachable: boolean;
  /** 0 = 連線階段就失敗 */
  status: number;
  latency_ms: number;
  message: string;
}

/** 故障轉移佇列裡的一列（＝某個模型在某個來源上的候選順序）。 */
export interface QueueRow {
  provider_id: number;
  provider_name: string;
  api_format: string;
  enabled: boolean;
  model_count: number;
  /** 這個來源對這個模型學到的協議（null = 還沒學到） */
  learned_format: string | null;
  /** 斷路器：冷卻中（會被排到候選最後） */
  open: boolean;
  cooldown_secs: number;
}

/** 可挑選的模型 + 有幾個來源登記它。 */
export interface QueueModel {
  model: string;
  sources: number;
}

// ── P2.1：每來源的用量查詢（對標 cc-switch 的 Usage Query） ──

export interface UsageHeader {
  name: string;
  value: string;
}

/** 一個來源的用量查詢設定。 */
export interface UsageQueryConfig {
  provider_id: number;
  enabled: boolean;
  /** 樣板 id（custom = 全部自己填） */
  template: string;
  /** 空 = 用來源自己的 */
  base_url: string;
  /** 空 = 用來源自己的 */
  api_key: string;
  access_token: string;
  user_id: string;
  /** 例：{{baseUrl}}/user/balance */
  url_template: string;
  method: string;
  headers: UsageHeader[];
  /** 抽取規格（見後端 usage_query/extract.rs 的說明） */
  extractor: unknown;
  timeout_secs: number;
  /** 自動查詢間隔（分）；0 = 不自動查 */
  auto_interval_min: number;
}

/** 內建樣板。 */
export interface UsageTemplate {
  id: string;
  name: string;
  /** 預期的回應形狀與注意事項（樣板猜錯比沒有更糟，所以一定要顯示） */
  note: string;
  config: {
    url_template: string;
    method: string;
    headers: [string, string][];
    extractor: unknown;
    timeout_secs: number;
    auto_interval_min: number;
    /** 需要使用者自己填的欄位 */
    needs: string[];
  };
}

/** 多方案裡的一個方案。 */
export interface QuotaPlan {
  name: string;
  remaining: number | null;
  used: number | null;
  total: number | null;
  unit: string;
}

/** 一次用量查詢的結果（不含任何金鑰）。 */
export interface QuotaView {  provider_id: number;
  ok: boolean;
  message: string;
  unit: string;
  remaining: number | null;
  used: number | null;
  total: number | null;
  /** 用量百分比（有 total 才算得出來；顏色判斷用這個） */
  percent: number | null;
  plan_name: string;
  extra: string;
  plans: QuotaPlan[];
  /** 原始回應（截斷；樣板對不上時要看它） */
  raw: string;
  checked_at: number;
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
  /** 佔位值：後端一律覆寫成網關 URL（唯一模式就是經網關）。 */
  base_url: string;
  api_key: string;
  model: string;
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

// ── P3.1：MCP 管理（對標 cc-switch 的 MCP Management） ──

/** 名稱／值配對（MCP 的標頭與環境變數共用）。 */
export interface McpPair {
  name: string;
  value: string;
}

/** 一個 MCP 伺服器對一個工具的啟用狀態。 */
export interface McpBinding {
  app: string;
  enabled: boolean;
}

export interface McpServer {
  id: number;
  /** Server ID（設定檔裡的鍵；唯一） */
  slug: string;
  name: string;
  description: string;
  /** stdio | http | sse */
  transport: string;
  command: string;
  args: string[];
  url: string;
  headers: McpPair[];
  env: McpPair[];
  bindings: McpBinding[];
}

export interface McpInput {
  id?: number | null;
  slug: string;
  name: string;
  description: string;
  transport: string;
  command: string;
  args: string[];
  url: string;
  headers: McpPair[];
  env: McpPair[];
}

export interface McpPreset {
  id: string;
  name: string;
  /** 套件名（照抄 cc-switch 手冊的表） */
  package: string;
  description: string;
  note: string;
  input: {
    slug: string;
    name: string;
    description: string;
    transport: string;
    command: string;
    args: string[];
    url: string;
  };
}

/** 某個工具的同步結果。 */
export interface McpSyncOutcome {
  app: string;
  display: string;
  /** 沒安裝就 true（cc-switch：不報錯，只是不寫） */
  skipped: boolean;
  message: string;
  path: string;
  written: string[];
  removed: string[];
  backup: string | null;
}

/** 匯入既有設定的報告（每個工具一列，失敗的附原因）。 */
export interface McpImportReport {
  added: number;
  skipped: number;
  per_app: { app: string; count: number; error: string }[];
  imported: string[];
}

// ── P3.2：提示詞預設集（對標 cc-switch 的 Prompts Management） ──

export interface PromptPreset {
  id: number;
  /** claude | codex | opencode */
  app: string;
  name: string;
  content: string;
  active: boolean;
  created_at: number;
  updated_at: number;
}

export interface PromptInput {
  id?: number | null;
  app: string;
  name: string;
  content: string;
}

export interface PromptApp {
  app: string;
  display: string;
  path: string;
  installed: boolean;
}

/** 面板狀態：檔案路徑、大小、啟用中的預設集、檔案是否被手改過。 */
export interface PromptPanelState {
  app: string;
  path: string;
  file_exists: boolean;
  live_bytes: number;
  active_id: number | null;
  active_name: string;
  /** 檔案內容與啟用中的預設集不同 → 切換時會先把它存回該預設集 */
  dirty: boolean;
}

/** 一次同步（含回填）的結果。 */
export interface PromptBackfill {
  app: string;
  path: string;
  backfilled_into: string;
  backfilled_bytes: number;
  wrote_file: boolean;
  backup: string | null;
  /** 首次啟動自動匯入的預設集名稱（沒有就空） */
  imported: string;
  message: string;
}

// ── P3.3：技能管理（對標 cc-switch 的 Skills Management） ──

export interface SkillRepo {
  id: number;
  owner: string;
  name: string;
  branch: string;
  subdir: string;
  label: string;
  /** 內建（不可刪除） */
  builtin: boolean;
}

export interface RemoteSkill {
  name: string;
  path: string;
  repo_id: number;
  repo_label: string;
  installed: boolean;
  update_available: boolean;
}

export interface SkillBinding {
  app: string;
  enabled: boolean;
}

export interface InstalledSkill {
  id: number;
  name: string;
  description: string;
  repo_id: number | null;
  repo_label: string;
  remote_path: string;
  content_hash: string;
  installed_at: number;
  updated_at: number;
  /** 母本目錄大小（bytes） */
  size: number;
  bindings: SkillBinding[];
}

export interface SkillSyncReport {
  skill: string;
  linked: string[];
  removed: string[];
  /** symlink 失敗、退回複製的目標（Windows 沒開開發者模式時很正常） */
  copied_fallback: string[];
  message: string;
}

export interface SkillInstallOutcome {
  skill: string;
  description: string;
  files: number;
  hash: string;
  storage: string;
  updated: boolean;
  unchanged: boolean;
  sync: SkillSyncReport;
}

export interface SkillDiscoverResult {
  skills: RemoteSkill[];
  /** 掃描失敗的儲存庫（名稱, 原因） */
  errors: [string, string][];
}

export interface SkillBackup {
  name: string;
  skill: string;
  at: string;
  bytes: number;
}

export interface SkillSettings {
  /** builtin | agents */
  storage: string;
  /** symlink | copy */
  sync_mode: string;
}

// ── P3.4：會話管理（對標 cc-switch 的 Session Manager） ──

export interface SessionApp {
  app: string;
  display: string;
}

export interface SessionSummary {
  app: string;
  id: string;
  title: string;
  project_dir: string;
  /** 來源檔（OpenCode 是資料庫路徑） */
  path: string;
  last_active_ms: number;
  size: number;
  /** 只有本機檔案才可刪 */
  deletable: boolean;
  /** 續聊指令（空＝不支援） */
  resume_command: string;
  message_count: number;
}

export interface SessionMessage {
  role: string;
  text: string;
  ts: number;
}

export interface SessionDeleteOutcome {
  deleted: string[];
  failed: [string, string][];
  removed_bytes: number;
  note: string;
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
  ok_requests: number;
  success_rate: number;
  in_tok: number;
  out_tok: number;
  cache_read: number;
  cache_write: number;
  tokens: number;
  /** 快取命中率，後端 `hit_rate()` 演算法，與 UsageSummary 同口徑。 */
  cache_hit_rate: number;
  cost_usd: number;
}

export interface ModelStat {
  model: string;
  requests: number;
  in_tok: number;
  out_tok: number;
  cache_read: number;
  cache_write: number;
  tokens: number;
  /** 快取命中率，後端 `hit_rate()` 演算法，與 UsageSummary 同口徑。 */
  cache_hit_rate: number;
  cost_usd: number;
}

/**
 * 分本機工具統計（「工具」視角）。唯一能並排比較各工具的來源。
 *
 * ⚠️ 整體命中率**不可**由本表各行自行平均：各工具 token 量差距極大，
 * 簡單平均與以 token 加權的正確值實測可差 19.7 個百分點。
 * 需要整體數字時一律用 `usageSummary`。
 */
export interface AppStat {
  app: string;
  requests: number;
  ok_requests: number;
  success_rate: number;
  in_tok: number;
  out_tok: number;
  cache_read: number;
  cache_write: number;
  tokens: number;
  /** 快取命中率，後端 `hit_rate()` 演算法，與 UsageSummary 同口徑。 */
  cache_hit_rate: number;
  cost_usd: number;
}

export * from "./apiTypesPricing";
export * from "./apiTypesDiagnostics";
