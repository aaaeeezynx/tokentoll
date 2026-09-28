// 型別／常數宣告：計價、模型目錄與歷史匯入（自 api.ts 原樣搬移，未改動任何內容）。

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
