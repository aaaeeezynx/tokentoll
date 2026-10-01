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
