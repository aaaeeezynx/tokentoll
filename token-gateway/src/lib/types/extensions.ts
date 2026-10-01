/** 擴充面板型別（用量查詢、MCP、提示詞、技能、會話）。 */

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
