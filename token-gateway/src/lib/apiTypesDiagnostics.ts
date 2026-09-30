// 型別／常數宣告：網關連接埠常數、gatewayUrl 與診斷中心（自 api.ts 原樣搬移，未改動任何內容）。

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

/** 網關學到的「這個來源的這個模型在哪種端點上架」。 */
export interface LearnedProtocol {
  provider_id: number;
  /** 空字串表示來源已被刪除 */
  provider_name: string;
  model: string;
  /** 來源設定宣告的協議 */
  declared_format: string;
  /** 實際上會通的協議（自動換手學到的） */
  actual_format: string;
}

/** 某個來源目前的健康狀態（斷路器）。 */
export interface SourceHealth {
  provider_id: number;
  /** 空字串表示來源已被刪除 */
  provider_name: string;
  api_format: string;
  /** true = 冷卻中（連續失敗達門檻，暫時排到候選最後） */
  open: boolean;
  consecutive_failures: number;
  /** 還要冷卻幾秒 */
  cooldown_secs: number;
  last_error: string;
  secs_since_failure: number | null;
  secs_since_success: number | null;
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
  /** 學到的上游協議（自動換手用，落庫、重啟仍有效） */
  learned_protocols: LearnedProtocol[];
}
