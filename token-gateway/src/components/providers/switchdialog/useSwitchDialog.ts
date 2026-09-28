//! 由 `components/providers/SwitchDialog.tsx` 拆分而來（原檔 574 行）。程式碼語意未改，只搬位置。
//! 對話框的狀態、查詢與套用流程集中於此，JSX 見 `./SwitchDialog`。

import {
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  useMutation,
  useQuery,
} from "@tanstack/react-query";
import {
  api,
  gatewayUrl,
  type SwitchRequest,
} from "../../../lib/api";
import {
  useConfirm,
} from "../../Confirm";
import {
  ClaudeTierKey,
  parseTierRaw,
} from "../ProviderForm";
import {
  loadLastSwitch,
  saveLastSwitch,
} from "../providersTypes";
import type {
  SwitchDialogProps,
} from "./shared";

export function useSwitchDialog(props: SwitchDialogProps) {
  const { tool, providers, port } = props;
  // 上次成功參數：對話框預填（行開關不再靜默重放，改在這裡一鍵確認）
  const saved = useMemo(() => loadLastSwitch(tool.app), [tool.app]);
  const [providerId, setProviderId] = useState<number | null>(() => {
    if (saved?.provider_id != null && providers.some((p) => p.id === saved.provider_id)) {
      return saved.provider_id;
    }
    return props.initialProviderId != null && providers.some((p) => p.id === props.initialProviderId)
      ? props.initialProviderId
      : providers.length > 0 ? providers[0].id : null;
  });
  const [rowId, setRowId] = useState<number>(0);
  const [manualModel, setManualModel] = useState("");
  // 舊存檔無 key_id：視為「不填」（用來源上游 Key），避免下拉與實際值不一致
  const [apiKey, setApiKey] = useState(
    saved?.key_id != null ? (saved?.api_key ?? "") : "",
  );
  // 本地 Key 下拉：存 id（secret 經 keyReveal 取回並快取），空 = 用來源上游 Key
  const [keyId, setKeyId] = useState<string>(
    saved?.key_id != null ? String(saved.key_id) : "",
  );
  // Claude Code 模型映射（五檔）：存 raw（見 parseTierRaw）
  const [claudeRaws, setClaudeRaws] = useState<Record<ClaudeTierKey, string>>(
    () => ({
      default: saved?.claude_map?.default ?? "",
      haiku: saved?.claude_map?.haiku ?? "",
      sonnet: saved?.claude_map?.sonnet ?? "",
      opus: saved?.claude_map?.opus ?? "",
      fable: saved?.claude_map?.fable ?? "",
    }),
  );
  const revealedKeys = useRef<Map<number, string>>(new Map());
  const localKeys = useQuery({
    queryKey: ["keys"],
    queryFn: api.keysList,
    staleTime: 30_000,
  });
  const enabledLocalKeys = useMemo(
    () => (localKeys.data || []).filter((k) => k.enabled),
    [localKeys.data],
  );
  const pickedKey = useMemo(
    () =>
      keyId === ""
        ? null
        : enabledLocalKeys.find((k) => String(k.id) === keyId) ?? null,
    [enabledLocalKeys, keyId],
  );
  const pickKey = (v: string) => {
    setKeyId(v);
    if (keyMsg) setKeyMsg("");
    if (v === "") {
      setApiKey("");
      return;
    }
    const id = Number(v);
    const k = enabledLocalKeys.find((x) => x.id === id);
    // 本地 Key 綁了來源 → 自動同步來源，避免 Key 與工具來源/模型衝突
    if (k?.provider_id != null && providers.some((p) => p.id === k.provider_id)) {
      setProviderId(k.provider_id);
    }
    const cached = revealedKeys.current.get(id);
    if (cached != null) {
      setApiKey(cached);
      return;
    }
    api
      .keyReveal(id)
      .then((secret) => {
        revealedKeys.current.set(id, secret);
        setApiKey(secret);
      })
      .catch((e) => setKeyMsg(`讀取本地 Key 失敗：${String(e)}`));
  };
  // 開場若上次選的是本地 Key：重新取回明文（key 可能已輪換，避免用舊值）
  useEffect(() => {
    if (keyId !== "") pickKey(keyId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // 開關初始跟隨該工具當前是否被網關接管
  const [viaGateway, setViaGateway] = useState(tool.gateway_active);
  const [genCatalog, setGenCatalog] = useState(saved?.gen_catalog ?? true);
  const [catalogUnion, setCatalogUnion] = useState(saved?.catalog_union ?? false);
  const [result, setResult] = useState<string>("");
  const { dialog: confirmDialog, ask } = useConfirm();

  const provider = useMemo(
    () => providers.find((p) => p.id === providerId) ?? null,
    [providers, providerId],
  );
  const umodels = useQuery({
    queryKey: ["umodels", providerId],
    queryFn: () => api.modelsList(providerId as number),
    enabled: providerId != null,
  });
  const enabledRows = useMemo(
    () => (umodels.data || []).filter((m) => m.enabled),
    [umodels.data],
  );
  useEffect(() => {
    setRowId(0);
    setManualModel("");
  }, [providerId]);
  useEffect(() => {
    if (rowId === 0 && manualModel.trim() === "" && enabledRows.length > 0) {
      setRowId(enabledRows[0].id);
    }
  }, [enabledRows, rowId, manualModel]);
  // 上次模型預填一次：按 actual_model 對行，否則落手動輸入
  const prefilled = useRef(false);
  useEffect(() => {
    if (prefilled.current || !saved?.model || enabledRows.length === 0) return;
    prefilled.current = true;
    const hit = enabledRows.find((m) => m.actual_model === saved.model);
    if (hit) setRowId(hit.id);
    else setManualModel(saved.model);
  }, [enabledRows, saved]);
  const row = enabledRows.find((m) => m.id === rowId) ?? null;
  const effModel = row ? row.actual_model : manualModel.trim();
  // 本地 Key 的模型白名單：過濾下拉，避免 Key 與來源/模型衝突
  const keyAllowedModels = pickedKey?.allowed_models ?? [];
  const modelRows = useMemo(() => {
    if (keyAllowedModels.length === 0) return enabledRows;
    const filtered = enabledRows.filter(
      (m) =>
        keyAllowedModels.includes(m.actual_model) ||
        keyAllowedModels.includes(m.display_name),
    );
    // 白名單與該來源模型無交集時回退全部（避免死路），提示交給下方文字
    return filtered.length > 0 ? filtered : enabledRows;
  }, [enabledRows, keyAllowedModels]);
  // 模型映射各檔位有效值：手動文本優先，留空跟隨主模型
  const claudeMapOpt = useMemo(() => {
    const eff = (k: ClaudeTierKey): string => {
      const { sel, txt } = parseTierRaw(
        claudeRaws[k],
        modelRows.map((m) => ({ value: m.actual_model })),
      );
      if (sel === "manual") return txt.trim() || effModel;
      return sel || effModel;
    };
    return {
      default: eff("default"),
      haiku: eff("haiku"),
      sonnet: eff("sonnet"),
      opus: eff("opus"),
      fable: eff("fable"),
    };
  }, [claudeRaws, modelRows, effModel]);
  // 多選推理等級：取第一個作為寫入配置的值
  const reasoningRaw = row && row.reasoning !== "unset" ? row.reasoning : null;
  const reasoning = reasoningRaw?.split(",")[0]?.trim() || null;
  const context = row?.context_window ?? null;
  // 只有一種路由模式：經本網關。base_url 由後端覆寫成網關 URL，這裡送的是佔位值。
  const buildReq = (): SwitchRequest | null =>
    provider && effModel
      ? {
          app: tool.app,
          base_url: gatewayUrl(port, tool.app),
          api_key: apiKey,
          model: effModel,
          provider_id: providerId,
          provider_format: provider?.api_format ?? null,
          reasoning,
          context_window: context,
          gen_catalog: genCatalog && tool.app === "codex",
          catalog_union: catalogUnion && tool.app === "codex",
          // Key 留空 = 直連上游 Key（後端注入；經網關不經本地 Key 轉發）
          direct_upstream: !apiKey.trim(),
          key_id: keyId === "" ? null : Number(keyId),
          claude_map: tool.app === "claude" ? claudeMapOpt : null,
        }
      : null;

  const req = buildReq();

  const plan = useQuery({
    queryKey: ["plan", tool.app, port, req],
    queryFn: () => api.switchPlan(req as SwitchRequest, port),
    enabled: req !== null,
  });

  const apply = useMutation({
    mutationFn: (r: SwitchRequest) => api.switchApply(r, port),
  });

  const closeTimer = useRef<number | null>(null);
  const [closing, setClosing] = useState(false);
  useEffect(() => {
    return () => {
      if (closeTimer.current) window.clearTimeout(closeTimer.current);
    };
  }, []);
  // 關閉動畫：先播 pop-out，180ms 後真正卸載
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    closeTimer.current = window.setTimeout(() => props.onClose(), 180);
  };
  const [keyMsg, setKeyMsg] = useState("");
  const [restoring, setRestoring] = useState(false);
  // 所有工具的 Key 欄皆可留空：留空 = 用所選來源的上游 Key（後端注入，
  // 經網關時不經本地 Key 轉發；直連時明文寫入工具配置）。
  // 本地 Key 白名單縮小模型表時，把選中項拉回第一個可用模型
  useEffect(() => {
    if (manualModel.trim() !== "" || modelRows.length === 0) return;
    if (!modelRows.some((m) => m.id === rowId)) setRowId(modelRows[0].id);
  }, [modelRows, rowId, manualModel]);

  const afterApply = (r: SwitchRequest, res: { config_path: string; backup_path: string | null; extra_files: string[] }) => {
    saveLastSwitch(tool.app, r);
    setResult(
      `已寫入 ${res.config_path}` +
        (res.backup_path ? `\n備份：${res.backup_path}` : "\n（新建檔案，無需備份）") +
        (res.extra_files.length > 0 ? `\n附帶：${res.extra_files.join(", ")}` : "") +
        "\n視窗即將自動關閉…",
    );
    props.onApplied();
    closeTimer.current = window.setTimeout(requestClose, 1400);
  };

  /**
   * 右上開關 = 開啟網關接管；關閉 = 還原接管前備份**並切回該工具的原生來源**。
   *
   * 關閉不是單純還原備份：備份可能是很久以前的快照，只還原會停在一個使用者
   * 早就不用的第三方來源（2026-09-28 實例：Codex 卡在 09-26 的 NIM 設定，
   * 模型清單裡只有 NIM 的模型）。詳見後端 `tools/native.rs`。
   */
  const flip = (v: boolean) => {
    if (!v) {
      ask(`關閉 ${tool.display} 的網關接管？`, () => doRestore(), {
        message:
          "將還原最近一次接管前的配置，並把來源切回這個工具的原生來源（Codex→OpenAI/GPT、Claude Code→Claude、OpenCode→自己的登入），流量不再經網關。",
        confirmLabel: "關閉接管",
      });
      return;
    }
    doFlip(true);
  };

  const doRestore = () => {
    setKeyMsg("");
    setResult("");
    setRestoring(true);
    api
      .switchOff(tool.app, port)
      .then((msg) => {
        setViaGateway(false);
        setResult(`${msg}\n視窗即將自動關閉…`);
        props.onApplied();
        closeTimer.current = window.setTimeout(requestClose, 1400);
      })
      .catch((e) => setResult(`失敗：${String(e)}`))
      .finally(() => setRestoring(false));
  };

  const doFlip = (v: boolean) => {
    const r = buildReq();
    if (!r) return;
    setKeyMsg("");
    setResult("");
    setViaGateway(v);
    apply.mutate(r, {
      onSuccess: (res) => afterApply(r, res),
      onError: (e) => {
        setViaGateway(!v);
        setResult(`失敗：${String(e)}`);
      },
    });
  };

  const inputCls =
    "field w-full px-3.5 py-2 text-sm";

  const switchDisabled = apply.isPending || restoring || plan.data?.supported === false;

  /** 直接套用當前下拉選擇（換來源/模型無需重啟開關；開關狀態不變，只重寫工具配置）。 */
  const applySelection = () => {
    const r = buildReq();
    if (!r) return;
    setKeyMsg("");
    setResult("");
    apply.mutate(r, {
      onSuccess: (res) => afterApply(r, res),
      onError: (e) => setResult(`失敗：${String(e)}`),
    });
  };

  return {
    closing,
    requestClose,
    tool,
    viaGateway,
    flip,
    switchDisabled,
    providers,
    providerId,
    setProviderId,
    enabledRows,
    modelRows,
    rowId,
    setRowId,
    inputCls,
    manualModel,
    setManualModel,
    row,
    claudeRaws,
    setClaudeRaws,
    effModel,
    keyId,
    pickKey,
    enabledLocalKeys,
    keyMsg,
    pickedKey,
    genCatalog,
    setGenCatalog,
    catalogUnion,
    setCatalogUnion,
    req,
    apply,
    plan,
    applySelection,
    result,
    confirmDialog,
  };
}
