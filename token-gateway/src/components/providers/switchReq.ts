/**
 * 「把某個工具切到某個來源」的請求組裝 —— **唯一一份實作**。
 *
 * 由 `useToolSwitch` 的 `resolveSelectionReq` 抽出來，因為現在有兩條路要用：
 * 工具卡片（行開關／詳情套用）與**系統匣切換**（P1.6）。
 * 托盤那一側刻意不在 Rust 重寫一份規則（挑模型、挑 Key 的規則一旦有兩份，
 * 兩邊遲早會不一致），而是讓 Rust 只發事件、由前端呼叫這裡。
 *
 * 規則（與原本一字不變）：
 * - **模型**：上次用的、否則工具現在用的、兩者都不在該來源的啟用清單裡就用第一個啟用模型。
 * - **Key**：沿用上次的偏好；留空＝以該來源的上游 Key 直連（後端注入，用量照常記錄）。
 */
import { api, gatewayUrl, type Provider, type SwitchRequest, type ToolStatus } from "../../lib/api";
import { loadLastSwitch } from "./providersTypes";

export async function buildSelectionReq(args: {
  app: string;
  provider: Provider;
  port: number;
  tools: ToolStatus[];
}): Promise<{ req: SwitchRequest; upstreamKeyWritten: boolean }> {
  const { app, provider, port, tools } = args;
  const saved = loadLastSwitch(app);
  const apiKey = saved?.api_key ?? "";
  const upstreamKeyWritten = !apiKey.trim();
  if (upstreamKeyWritten && !(provider.api_key ?? "").trim()) {
    throw new Error(`來源 ${provider.name} 未填寫上游 Key，無法直連`);
  }
  const models = await api.modelsList(provider.id);
  const enabled = models.filter((m) => m.enabled);
  if (enabled.length === 0) {
    throw new Error(`來源 ${provider.name} 沒有啟用的模型`);
  }
  const currentModel = (tools || []).find((x) => x.app === app)?.current_model ?? null;
  const candidates = [saved?.model, currentModel].filter(Boolean) as string[];
  const hit = enabled.find((m) => candidates.includes(m.actual_model)) ?? null;
  const row = hit ?? enabled[0];
  const reasoning =
    row.actual_model === saved?.model && saved?.reasoning
      ? saved.reasoning
      : row.reasoning !== "unset"
        ? (row.reasoning.split(",")[0]?.trim() || null)
        : null;
  return {
    req: {
      app,
      base_url: gatewayUrl(port, app),
      api_key: apiKey,
      model: row.actual_model,
      provider_id: provider.id,
      provider_format: provider.api_format ?? null,
      reasoning,
      context_window: row.context_window ?? null,
      gen_catalog: (saved?.gen_catalog ?? true) && app === "codex",
      catalog_union: saved?.catalog_union ?? false,
      direct_upstream: !apiKey.trim(),
      key_id: saved?.key_id ?? null,
    },
    upstreamKeyWritten,
  };
}
