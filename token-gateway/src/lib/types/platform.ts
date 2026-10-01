/** 平台面型別（Deep Link、外觀與啟動、視窗行為、資料庫備份）。 */

// ── P4.1：Deep Link 一鍵匯入（對標 cc-switch 的 ccswitch:// 協定） ──

export interface DeeplinkPreview {
  /** provider | prompt | mcp | skill */
  kind: string;
  title: string;
  /** [欄位名, 值]（機密值已遮罩） */
  fields: [string, string][];
  warnings: string[];
  effects: string[];
}

export interface DeeplinkApplied {
  kind: string;
  created: string[];
  skipped: string[];
  note: string;
}

export interface DeeplinkProtocolState {
  scheme: string;
  registered: boolean;
  command: string;
}

// ── P4.3／P4.4：外觀與開機自啟 ──

export interface AppearanceState {
  /** system | light | dark */
  theme: string;
  /** zh-TW | en */
  lang: string;
  autostart: boolean;
  autostart_command: string;
  exe: string;
}

// ── P4.7：視窗與托盤行為（關閉縮到系統匣、輕量模式） ──

export interface WindowBehavior {
  /** tray（預設，縮到系統匣）| exit（直接結束） */
  close_action: string;
  /** 現在是不是處於輕量模式（沒有視窗） */
  lightweight: boolean;
  autostart: boolean;
}

// ── P4.8：資料庫備份管理 ──

export interface DbBackupInfo {
  name: string;
  path: string;
  created_ms: number;
  bytes: number;
}

export interface DbBackupState {
  dir: string;
  interval_hours: number;
  retention: number;
  interval_options: number[];
  retention_options: number[];
  backups: DbBackupInfo[];
  last_ms: number;
}

export interface DbRestoreOutcome {
  restored: string;
  /** 還原前自動建立的安全備份 */
  safety_backup: string;
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

export * from "../apiTypesPricing";
export * from "../apiTypesDiagnostics";

// ── P4.10：雲端同步（WebDAV） ──

export interface WebdavConfig {
  base_url: string;
  username: string;
  /** UI 顯示的是遮罩後的值；留空送出＝不變更 */
  password: string;
  remote_root: string;
  profile: string;
}

export interface CloudState {
  /** webdav */
  provider: string;
  config: WebdavConfig;
  has_password: boolean;
  auto_sync: boolean;
  last_sync_ms: number;
  last_result: string;
  last_ok: boolean;
  local_mtime_ms: number;
  local_bytes: number;
  /** 自上次上傳後本地有變動 */
  pending_changes: boolean;
}

export interface RemoteSnapshot {
  protocol: string;
  schema_version: number;
  app_version: string;
  timestamp_ms: number;
  bytes: number;
  incompatible: boolean;
}

export interface CloudSyncOutcome {
  action: string;
  bytes: number;
  remote: string;
  note: string;
  snapshot: RemoteSnapshot | null;
}
