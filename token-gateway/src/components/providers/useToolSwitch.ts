import { useEffect, useRef, useState } from "react";
import {
  api,
  gatewayUrl,
  type Provider,
  type SwitchRequest,
  type ToolStatus,
} from "../../lib/api";
import { loadLastSwitch, saveLastSwitch } from "./providersTypes";

/**
 * 「本機工具」卡片的接管／還原行為。
 *
 * 由 `Providers.tsx` 抽出（該檔 2026-09-28 時 430 行）—— 這一整塊是
 * 「開關 ON/OFF、按來源組裝請求、寫入並回報」。
 * 抽出的界線：hook 只依賴傳入的 `deps`（來源清單、埠、所選來源），
 * 不碰任何 JSX。行為與文案一字未改。
 *
 * **2026-09-28 移除「來源切換自動重接管」**：原本所選來源一變就自動重寫
 * `config.toml`（永不彈框），導致「只是點一下來源卡片看模型」也會靜默改檔。
 * 現在寫入只發生在使用者明確動作（行開關／詳情「套用」）。見 `docs/TESTING.md`
 * §0.5.2 與 §0.5.3。
 */
export type ToolSwitch = {
  /** 正在切換中的工具 app 名（用來 disable 開關）。 */
  toggling: string | null;
  /** 行內錯誤訊息（單一工具）。 */
  rowErr: { app: string; msg: string } | null;
  /** 行內成功訊息（單一工具）。 */
  rowMsg: { app: string; msg: string } | null;
  /** 行開關的 onChange。 */
  toggleRow: (t: ToolStatus, on: boolean) => void;
};

export function useToolSwitch(deps: {
  port: number;
  /** 目前所有來源。 */
  providers: Provider[];
  /** 目前選取的來源 id（開關 ON 時若無上次參數則用它）。 */
  selId: number | null;
  /** 目前偵測到的工具（用來判斷 Codex 是否正被接管）。 */
  tools: ToolStatus[];
  /** 接管／還原成功後刷新查詢。 */
  refreshTools: () => void;
}): ToolSwitch {
  const { port, providers, selId, tools, refreshTools } = deps;
  const [toggling, setToggling] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<{ app: string; msg: string } | null>(null);
  const [rowMsg, setRowMsg] = useState<{ app: string; msg: string } | null>(null);

  // 這些是 window/effect 會讀到的最新值，用 ref 避免閉包拿到舊的。
  const latest = useRef({ port, providers, tools, selId });
  latest.current = { port, providers, tools, selId };

  /** 執行接管寫入（開關/自動共用）：存檔＋行內成功信息＋刷新；失敗只報行內錯誤。 */
  const doApply = async (
    t: ToolStatus,
    req: SwitchRequest,
    okPrefix: string,
    upstreamKeyWritten: boolean,
  ) => {
    setToggling(t.app);
    try {
      await api.switchApply(req, latest.current.port);
      saveLastSwitch(t.app, req);
      const pname =
        (latest.current.providers || []).find((p) => p.id === req.provider_id)?.name ?? "";
      const keyNote = upstreamKeyWritten
        ? "（直連上游 Key 已寫入配置文件，僅本機可讀）"
        : "";
      setRowMsg({
        app: t.app,
        msg: `${okPrefix}：${req.model}${pname ? ` @ ${pname}` : ""}${keyNote}`,
      });
      refreshTools();
    } catch (e) {
      setRowErr({ app: t.app, msg: String(e) });
    } finally {
      setToggling(null);
    }
  };

  /**
   * 按指定來源組裝接管請求（開關無參數回退 / 來源切換自動重接管共用，永不彈框）。
   * 模型：上次/現行仍在該來源啟用表則沿用，否則首個啟用模型；
   * Key：所有工具一致可空（留空 = 以該來源上游 Key 直連，後端注入；用量照常記錄）。
   */
  const resolveSelectionReq = async (
    t: ToolStatus,
    provider: Provider,
  ): Promise<{ req: SwitchRequest; upstreamKeyWritten: boolean }> => {
    const saved = loadLastSwitch(t.app);
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
    const currentModel =
      (latest.current.tools || []).find((x) => x.app === t.app)?.current_model ?? null;
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
        app: t.app,
        base_url: gatewayUrl(latest.current.port, t.app),
        api_key: apiKey,
        model: row.actual_model,
        provider_id: provider.id,
        provider_format: provider.api_format ?? null,
        reasoning,
        context_window: row.context_window ?? null,
        gen_catalog: (saved?.gen_catalog ?? true) && t.app === "codex",
        catalog_union: saved?.catalog_union ?? false,
        direct_upstream: !apiKey.trim(),
        key_id: saved?.key_id ?? null,
      },
      upstreamKeyWritten,
    };
  };

  /** 開關 ON：上次參數可用則沿用來源/模型/Key 偏好，但路由強制為本網關。 */
  const toggleOn = async (t: ToolStatus) => {
    const saved = loadLastSwitch(t.app);
    const savedKey = saved?.api_key ?? "";
    if (saved) {
      const req: SwitchRequest = {
        ...saved,
        app: t.app,
        base_url: gatewayUrl(latest.current.port, t.app),
        api_key: savedKey,
        gen_catalog: saved.gen_catalog ?? true,
        catalog_union: saved.catalog_union ?? false,
        direct_upstream: !savedKey.trim(),
      };
      await doApply(t, req, "已接管", req.direct_upstream);
      return;
    }
    const provider = (latest.current.providers || []).find(
      (p) => p.id === latest.current.selId,
    );
    if (!provider) {
      setRowErr({ app: t.app, msg: "請先選擇一個上游來源" });
      return;
    }
    try {
      const { req, upstreamKeyWritten } = await resolveSelectionReq(t, provider);
      await doApply(t, req, "已接管", upstreamKeyWritten);
    } catch (e) {
      setRowErr({ app: t.app, msg: e instanceof Error ? e.message : String(e) });
    }
  };

  /** 行開關（只開/關，永不彈框）：關=還原接管前備份；開=按上次來源/模型強制經網關接管。詳情設定一律點行左側進入。 */
  const toggleRow = (t: ToolStatus, on: boolean) => {
    setRowErr(null);
    setRowMsg(null);
    if (!on) {
      setToggling(t.app);
      api
        .switchRestore(t.app, latest.current.port)
        .then((msg) => {
          setRowMsg({ app: t.app, msg });
          refreshTools();
        })
        .catch((e) => setRowErr({ app: t.app, msg: String(e) }))
        .finally(() => setToggling(null));
      return;
    }
    void toggleOn(t);
  };

  // 所選來源改變時**只提示、不寫檔**。
  //
  // 這裡以前會「來源切換自動重接管」（註解原文寫著「永不彈框」）—— 代價是：
  // 只是在來源清單點一下卡片看看它的模型與定價，就會**靜默重寫**
  // `~/.codex/config.toml`（並輪換備份、可能換掉舊會話用的上游），使用者
  // 完全不會知道。2026-09-28 發生過真實事故，見 `docs/TESTING.md` §0.5.2。
  //
  // 現在改成：**寫入一律要使用者的明確動作** —— 行開關，或進詳情按「套用」。
  // 這一步仍然偵測「選了別的來源」，但只發訊息告訴使用者怎麼套用。
  const prevSelId = useRef<number | null>(null);
  useEffect(() => {
    if (prevSelId.current === null) {
      prevSelId.current = selId;
      return;
    }
    if (prevSelId.current === selId) return;
    prevSelId.current = selId;
    if (selId == null) return;
    const codex = (tools || []).find((x) => x.app === "codex");
    if (!codex?.gateway_active || toggling === "codex") return;
    const name = (latest.current.providers || []).find((p) => p.id === selId)?.name;
    setRowErr(null);
    setRowMsg({
      app: "codex",
      msg: `已選擇來源「${name ?? selId}」，但 Codex 仍接管自原來源（未改動 config.toml）。要換過去請按右側開關，或點此行左側進詳情按「套用」`,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selId, tools, toggling]);

  return { toggling, rowErr, rowMsg, toggleRow };
}
