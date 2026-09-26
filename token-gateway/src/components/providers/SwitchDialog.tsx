//! 由 `Providers.tsx` 拆分而來（原檔 2,845 行）。程式碼語意未改，只搬位置。

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
  type Provider,
  type PricingMode,
  type SwitchRequest,
  type ToolStatus,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  PopSelect,
} from "../PopSelect";
import {
  Toggle,
} from "../Toggle";
import {
  useConfirm,
} from "../Confirm";
import {
  Logo,
} from "../logos";
import {
  CLAUDE_TIERS,
  ClaudeTierField,
  ClaudeTierKey,
  parseTierRaw,
} from "./ProviderForm";
import {
  copyText,
  loadLastSwitch,
  saveLastSwitch,
} from "./providersTypes";

export function SwitchDialog(props: {
  tool: ToolStatus;
  providers: Provider[];
  port: number;
  onClose: () => void;
  onApplied: () => void;
    /** 呼叫時來源列表的選中項：對話框預設跟隨它，而非固定取第一個來源 */
  initialProviderId: number | null;
}) {  const { tool, providers, port } = props;
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
  const buildReq = (via: boolean): SwitchRequest | null =>
    provider && effModel
      ? {
          app: tool.app,
          base_url: via ? gatewayUrl(port, tool.app) : provider.base_url,
          api_key: apiKey,
          model: effModel,
          via_gateway: via,
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

  const req = buildReq(true);

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

  /** 右上開關 = 開啟網關接管；關閉還原接管前配置。 */
  const flip = (v: boolean) => {
    if (!v) {
      ask(`關閉 ${tool.display} 的網關接管？`, () => doRestore(), {
        message: "將還原最近一次接管前的配置，流量不再經網關。",
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
      .switchRestore(tool.app, port)
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
    const r = buildReq(true);
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
    const r = buildReq(true);
    if (!r) return;
    setKeyMsg("");
    setResult("");
    apply.mutate(r, {
      onSuccess: (res) => afterApply(r, res),
      onError: (e) => setResult(`失敗：${String(e)}`),
    });
  };
  return (
    <div
      className={`fixed inset-0 z-[80] flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div
        className={`max-h-[90vh] w-full max-w-md overflow-y-auto overflow-x-clip glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}
      >
        <div className="mb-4 flex items-center gap-2">
          <button
            className="rounded-full bg-white/[0.055] p-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <Logo name={tool.app} size={22} className="shrink-0" />
          <div className="flex-1 text-[15px] font-semibold tracking-tight text-white">
            切換 {tool.display}
          </div>
          <Toggle
            checked={viaGateway}
            onChange={flip}
            disabled={switchDisabled}
            title="經本地網關：開=寫入網關接管，關=還原接管前配置"
          />
        </div>
        {providers.length === 0 ? (
          <p className="text-sm text-white/40">
            還沒有來源，請先在左側添加一個上游來源。
          </p>
        ) : (
          <div className="space-y-2.5">
            <PopSelect
              label="來源"
              value={providerId != null ? String(providerId) : ""}
              onChange={(v) => setProviderId(Number(v))}
              options={providers.map((p) => ({
                value: String(p.id),
                label: p.name,
                hint: p.base_url,
              }))}
            />
            {enabledRows.length > 0 ? (
              <>
                <PopSelect
                  label="模型"
                  searchable
                  value={String(rowId)}
                  onChange={(v) => setRowId(Number(v))}
                  options={[
                    ...modelRows.map((m) => ({
                      value: String(m.id),
                      label: m.display_name,
                      hint:
                        m.display_name !== m.actual_model
                          ? m.actual_model
                          : undefined,
                    })),
                    { value: "0", label: "手動輸入…" },
                  ]}
                />
                {rowId === 0 && (
                  <input
                    className={inputCls}
                    placeholder="實際請求模型 ID"
                    value={manualModel}
                    onChange={(e) => setManualModel(e.target.value)}
                  />
                )}
                {(row || manualModel.trim()) && (
                  <p className="font-mono text-xs text-white/40">
                    → {row ? row.actual_model : manualModel.trim()}
                    {row?.context_window
                      ? ` · ${row.context_window.toLocaleString()} ctx`
                      : ""}
                  </p>
                )}
              </>
            ) : (
              <input
                className={inputCls}
                placeholder="模型 ID（該來源尚無可用模型，請先到詳情頁新增）"
                value={manualModel}
                onChange={(e) => setManualModel(e.target.value)}
              />
            )}
            {tool.app === "claude" && (
              <div className="space-y-2.5 rounded-xl border border-white/[0.07] p-3">
                <p className="text-[12px] font-semibold text-white/60">
                  模型映射
                </p>
                <p className="-mt-1.5 text-[11px] text-white/30">
                  留空檔位跟隨上方選擇的模型，不寫入；其他工具不受影響。
                </p>
                {CLAUDE_TIERS.map((t) => (
                  <ClaudeTierField
                    key={t.key}
                    label={t.label}
                    options={modelRows.map((m) => ({
                      value: m.actual_model,
                      label: m.display_name,
                      hint:
                        m.display_name !== m.actual_model
                          ? m.actual_model
                          : undefined,
                    }))}
                    raw={claudeRaws[t.key]}
                    followedBy={effModel}
                    onRaw={(v) =>
                      setClaudeRaws((p) => ({ ...p, [t.key]: v }))
                    }
                  />
                ))}
              </div>
            )}
            <div>
              <PopSelect
                label="本地 Key"
                value={keyId}
                onChange={pickKey}
                options={[
                  {
                    value: "",
                    label: "不填：使用所選來源的上游 Key",
                    hint: "用量照常記錄",
                  },
                  ...enabledLocalKeys.map((k) => ({
                    value: String(k.id),
                    label: `${k.name}（${k.key_prefix}）`,
                    hint:
                      (k.provider_name ? `來源：${k.provider_name}` : "任意來源") +
                      (k.rate_limit_qpm > 0 ? ` · 限流 ${k.rate_limit_qpm}/分` : ""),
                  })),
                ]}
              />
              {keyMsg && <p className="mt-1 text-xs text-amber-400">{keyMsg}</p>}
              <p className="mt-1 text-[11px] text-white/30">
                {pickedKey
                    ? "經網關時以本地 Key 計量與限流；開關關閉則還原接管前配置。"
                  : "留空 = 使用所選來源的上游 Key（不經本地 Key 轉發，用量照常記錄）"}
              </p>
            </div>
            {tool.app === "codex" && (
              <label className="flex items-center gap-2 text-[13px] text-white/60">
                <input
                  type="checkbox"
                  checked={genCatalog}
                  onChange={(e) => setGenCatalog(e.target.checked)}
                />
                同時產生 Codex 模型目錄
              </label>
            )}
            {tool.app === "codex" && genCatalog && (
              <label className="flex items-center gap-2 text-[13px] text-white/60">
                <input
                  type="checkbox"
                  checked={catalogUnion}
                  onChange={(e) => setCatalogUnion(e.target.checked)}
                />
                目錄包含其他來源模型（預設僅當前來源）
              </label>
            )}
            <button
              className="btn-primary w-full px-4 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-40"
              disabled={!req || apply.isPending || plan.data?.supported === false}
              title="直接套用當前選擇的來源與模型，無需關閉再重開開關"
              onClick={applySelection}
            >
              {apply.isPending ? "套用中…" : "套用（切換來源/模型）"}
            </button>
            <div className="border-t border-white/[0.07]" />
            <div className="text-xs">
              {plan.isPending && <p className="text-white/30">正在生成方案…</p>}
              {plan.isError && (
                <p className="text-red-400">{String(plan.error)}</p>
              )}
              {plan.data && (
                <div className="space-y-2">
                  <p className="truncate font-mono text-white/40" title={plan.data.config_path}>
                    {plan.data.config_path}
                    <span className="ml-1.5 text-white/25">
                      {plan.data.will_backup ? "· 自動備份" : "· 將新建"}
                    </span>
                  </p>
                  {plan.data.edits.length > 0 && (
                    <details>
                      <summary className="cursor-pointer text-white/50">
                        {plan.data.edits.length} 項寫入
                      </summary>
                      <div className="mt-1.5 space-y-1">
                        {plan.data.edits.map((e) => (
                          <p key={e} className="text-white/45">
                            • {e}
                          </p>
                        ))}
                      </div>
                    </details>
                  )}
                  {plan.data.env_setup.map((e) => (
                    <div key={e} className="flex items-start gap-2">
                      <p className="flex-1 break-all font-mono text-amber-300">{e}</p>
                      <button
                        className="btn-ghost shrink-0 rounded-md px-2 py-0.5 text-xs"
                        onClick={() => copyText(e)}
                      >
                        複製
                      </button>
                    </div>
                  ))}
                  {plan.data.warnings.map((w) => (
                    <p key={w} className="flex items-start gap-1.5 text-amber-400">
                      <Icon name="alert" size={13} className="mt-0.5 shrink-0" />
                      {w}
                    </p>
                  ))}
                </div>
              )}
            </div>
            {result && (
              <p className="break-all whitespace-pre-wrap text-xs text-[#30d158]">
                {result}
              </p>
            )}
          </div>
        )}
      </div>
      {confirmDialog}
    </div>
  );
}

export const MODE_LABEL: Record<PricingMode, string> = {
  usage: "按量",
  subscription: "訂閱",
  tou: "峰谷",
};
