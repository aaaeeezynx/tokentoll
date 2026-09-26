import { useEffect, useMemo, useRef, useState, type PointerEvent as RPointerEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  gatewayUrl,
  useGatewayPort,
  type PricingMode,
  type PricingPeriod,
  type PricingPeriodInput,
  type Provider,
  type ProviderInput,
  type ProviderPricing,
  type ProviderPricingInput,
  REASONING_OPTIONS,
  type SwitchRequest,
  type ToolStatus,
  type UsableModel,
  type UsableModelInput,
} from "../lib/api";
import GlobalPricingManager from "./Pricing";
import { Icon } from "./icons";
import { PopSelect } from "./PopSelect";
import { Segmented } from "./Segmented";
import { Toggle } from "./Toggle";
import { useConfirm } from "./Confirm";
import { Logo } from "./logos";

const APP_TYPES = ["universal", "claude", "codex", "opencode", "hermes", "dsh"];
const API_FORMATS = ["openai-chat", "openai-responses", "mixed", "anthropic", "gemini"];
const AUTH_SCHEMES = [
  { id: "bearer", label: "Bearer（預設）" },
  { id: "goog-key", label: "Google API Key" },
  { id: "anthropic", label: "Anthropic API Key" },
];

const EMPTY_INPUT: ProviderInput = {
  name: "",
  app_type: "universal",
  api_format: "openai-chat",
  base_url: "",
  api_key: "",
  auth_scheme: "bearer",
  models: [],
  priority: 0,
  enabled: true,
};

function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

/** 上次成功切換的參數（行開關一鍵重開用；存本機 localStorage）。 */
const LAST_KEY = (app: string) => `tg:lastSwitch:v1:${app}`;
function saveLastSwitch(app: string, req: SwitchRequest) {
  try {
    localStorage.setItem(
      LAST_KEY(app),
      JSON.stringify({ ...req, via_gateway: true, base_url: "" }),
    );
  } catch {
    /* 忽略配額錯誤 */
  }
}
function loadLastSwitch(app: string): SwitchRequest | null {
  try {
    const raw = localStorage.getItem(LAST_KEY(app));
    if (!raw) return null;
    const r = JSON.parse(raw) as SwitchRequest;
    if (!r || r.app !== app || !r.model) return null;
    return r;
  } catch {
    return null;
  }
}



function ProviderForm(props: {
  initial: Provider | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const qc = useQueryClient();
  const [form, setForm] = useState<ProviderInput>(() =>
    props.initial
      ? {
          name: props.initial.name,
          app_type: props.initial.app_type,
          api_format: props.initial.api_format,
          base_url: props.initial.base_url,
          api_key: props.initial.api_key,
          auth_scheme: props.initial.auth_scheme || "bearer",
          models: props.initial.models,
          priority: props.initial.priority,
          enabled: props.initial.enabled,
        }
      : EMPTY_INPUT,
  );
  const [err, setErr] = useState("");
  const [showKey, setShowKey] = useState(false);
  // 新建模式：本地草稿模型（直接顯示，無需先儲存）
  const [draftModels, setDraftModels] = useState<string[]>([]);
  const [draftInput, setDraftInput] = useState("");
  const [draftFetching, setDraftFetching] = useState(false);
  const [draftMsg, setDraftMsg] = useState("");
  const [created, setCreated] = useState<Provider | null>(null);
  const eff = created ?? props.initial;

  const save = useMutation({
    mutationFn: () => {
      const input: ProviderInput = {
        ...form,
        models: eff ? eff.models : [],
      };
      return eff
        ? api.providerUpdate(eff.id, input)
        : api.providerAdd(input);
    },
    onSuccess: async (p) => {
      void qc.invalidateQueries({ queryKey: ["providers"] });
      if (!eff && draftModels.length > 0) {
        // 將草稿模型批量寫入新建來源
        for (let i = 0; i < draftModels.length; i++) {
          const m = draftModels[i].trim();
          if (!m) continue;
          try {
            await api.modelAdd({
              provider_id: p.id,
              display_name: m,
              actual_model: m,
              context_window: null,
              reasoning: "unset",
              ord: i * 10,
              enabled: true,
            });
          } catch {}
        }
        void qc.invalidateQueries({ queryKey: ["umodels", p.id] });
        void qc.invalidateQueries({ queryKey: ["model_counts"] });
      }
      if (eff) {
        props.onDone();
      } else {
        setCreated(p);
        setDraftModels([]);
      }
    },
    onError: (e) => setErr(String(e)),
  });

  const handleDraftFetch = async () => {
    if (!form.name.trim()) {
      setErr("請先填寫來源名稱");
      return;
    }
    if (!form.base_url.trim()) {
      setErr("請先填寫上游地址");
      return;
    }
    if (!form.api_key.trim()) {
      setErr("請先填寫 API Key");
      return;
    }
    setDraftFetching(true);
    setDraftMsg("");
    try {
      const p = await api.providerAdd({
        ...form,
        models: [],
      });
      for (let i = 0; i < draftModels.length; i++) {
        const m = draftModels[i].trim();
        if (!m) continue;
        try {
          await api.modelAdd({
            provider_id: p.id,
            display_name: m,
            actual_model: m,
            context_window: null,
            reasoning: "unset",
            ord: i * 10,
            enabled: true,
          });
        } catch {}
      }
      setCreated(p);
      setDraftModels([]);
      setDraftInput("");
      void qc.invalidateQueries({ queryKey: ["providers"] });
      void qc.invalidateQueries({ queryKey: ["model_counts"] });
      try {
        await api.catalogFetch(p.id, form.api_key);
        void qc.invalidateQueries({ queryKey: ["catalog", p.id] });
        setDraftMsg("已建立來源並取得模型清單");
      } catch (e) {
        setDraftMsg(String(e));
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setDraftFetching(false);
    }
  };

  const set = <K extends keyof ProviderInput>(k: K, v: ProviderInput[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const fieldCls = "field w-full px-3 py-1.5 text-[13px]";
  return (
    <div className="space-y-4">
      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
          基本資訊
        </div>
        <div className="mac-frow">
          <span className="mac-cap">來源名稱</span>
          <input
            className={fieldCls}
            placeholder="如 ChatGPT"
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">類型</span>
          <PopSelect
            value={form.app_type}
            onChange={(v) => set("app_type", v)}
            options={APP_TYPES.map((a) => ({ value: a, label: a }))}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">接口格式</span>
          <PopSelect
            value={form.api_format}
            onChange={(v) => set("api_format", v)}
            options={API_FORMATS.map((a) => ({ value: a, label: a }))}
          />
        </div>
      </div>

      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
          連線
        </div>
        <div className="mac-frow">
          <span className="mac-cap">上游地址</span>
          <input
            className={fieldCls}
            placeholder="base_url，如 https://api.openai.com/v1"
            value={form.base_url}
            onChange={(e) => set("base_url", e.target.value)}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">鑑權方式</span>
          <PopSelect
            value={form.auth_scheme}
            onChange={(v) => set("auth_scheme", v)}
            options={AUTH_SCHEMES.map((a) => ({
              value: a.id,
              label: a.label,
            }))}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">API Key</span>
          <div className="relative">
            <input
              className={`${fieldCls} pr-9`}
              type={showKey ? "text" : "password"}
              autoComplete="new-password"
              spellCheck={false}
              placeholder="明文存本地庫，M3 加固"
              value={form.api_key}
              onChange={(e) => set("api_key", e.target.value)}
            />
            <button
              type="button"
              className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-1 text-white/30 transition-colors hover:text-white/70"
              title={showKey ? "隱藏" : "顯示"}
              onClick={() => setShowKey((v) => !v)}
            >
              <Icon name={showKey ? "eye-slash" : "eye"} size={15} />
            </button>
          </div>
        </div>
      </div>

      <div className="form-section">
        <div className="mb-1 flex items-center gap-2 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
          <span>使用模型</span>
          {!eff && draftModels.length > 0 && (
            <span className="ml-auto rounded-full bg-white/[0.08] px-2 py-0.5 text-[10px] font-medium text-white/50">
              {draftModels.length}
            </span>
          )}
        </div>
        {eff ? (
          <ModelPanel
            key={eff.id}
            provider={eff}
            rowsClassName="max-h-[38vh]"
            apiKeyOverride={form.api_key}
            baseUrlOverride={form.base_url}
          />
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
                disabled={draftFetching}
                onClick={() => void handleDraftFetch()}
              >
                <Icon name="download" size={11} />
                {draftFetching ? "建立中…" : "取得模型清單"}
              </button>
              <span className="text-[11px] text-white/25">貼上 API Key 後直接抓取，無需先點儲存</span>
            </div>
            {draftMsg && <p className="text-xs text-amber-300">{draftMsg}</p>}
            {draftModels.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {draftModels.map((m) => (
                  <span
                    key={m}
                    className="flex items-center gap-1 rounded-full bg-white/[0.07] px-2.5 py-1 text-xs text-white/70"
                  >
                    <span className="font-mono">{m}</span>
                    <button
                      type="button"
                      className="rounded-full p-0.5 text-white/40 hover:bg-white/10 hover:text-white"
                      onClick={() => setDraftModels((a) => a.filter((x) => x !== m))}
                      title="移除"
                    >
                      <Icon name="x" size={10} />
                    </button>
                  </span>
                ))}
              </div>
            )}
            <div className="flex gap-1.5">
              <input
                className="field w-full px-3 py-1.5 text-xs"
                placeholder="模型 ID，如 deepseek-v4-flash"
                value={draftInput}
                onChange={(e) => setDraftInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    const v = draftInput.trim();
                    if (v && !draftModels.includes(v)) {
                      setDraftModels((a) => [...a, v]);
                      setDraftInput("");
                    }
                  }
                }}
              />
              <button
                type="button"
                className="btn-ghost flex shrink-0 items-center gap-1 px-3 py-1.5 text-xs"
                onClick={() => {
                  const v = draftInput.trim();
                  if (v && !draftModels.includes(v)) {
                    setDraftModels((a) => [...a, v]);
                    setDraftInput("");
                  }
                }}
              >
                <Icon name="plus" size={11} />
                新增
              </button>
            </div>
            <p className="text-[11px] text-white/25">
              以上 {draftModels.length} 個將在儲存時一併建立；建立後可在此抓取清單、啟用/刪除。
            </p>
          </div>
        )}
      </div>

      {err && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs text-red-400">
          {err}
        </div>
      )}

      <div className="flex items-center justify-end gap-2.5 border-t border-white/[0.06] pt-3">
        <button
          className="btn-ghost px-5 py-1.5 text-[13px]"
          onClick={props.onCancel}
        >
          取消
        </button>
        <button
          className="btn-primary px-6 py-1.5 text-[13px] disabled:opacity-40"
          disabled={save.isPending}
          onClick={() => save.mutate()}
        >
          {save.isPending ? "儲存中…" : "儲存"}
        </button>
      </div>
    </div>
  );
}

/** 來源設定 sheet（與本機工具切換卡同設計語言）。 */
function ProviderSheet(props: {
  initial: Provider | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [closing, setClosing] = useState(false);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 180);
  };
  return (
    <div
      className={`fixed inset-0 z-[80] flex justify-center overflow-y-auto p-4 glass-backdrop transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div
        className={`my-auto flex max-h-[min(88vh,720px)] w-full max-w-lg flex-col overflow-hidden glass-panel ${closing ? "sheet-out" : "pop-in"}`}
      >
        <div className="flex shrink-0 items-center gap-3 border-b border-white/[0.06] bg-[rgba(24,24,30,0.98)] px-5 py-4">
          <button
            className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/[0.1] hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/65">
            <Icon name="server" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-white">
            {props.initial ? `編輯「${props.initial.name}」` : "新增來源"}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto overflow-x-clip px-5 py-5">
          <ProviderForm
            key={props.initial?.id ?? "new"}
            initial={props.initial}
            onDone={props.onSaved}
            onCancel={requestClose}
          />
        </div>
      </div>
    </div>
  );
}

function ToolVersions() {
  const qc = useQueryClient();
  const vers = useQuery({
    queryKey: ["tool_versions"],
    queryFn: api.toolVersions,
    staleTime: 60_000,
  });
  // 最新版走 registry，慢查詢獨立跑，不擋版本顯示。
  const latestQ = useQuery({
    queryKey: ["tool_latest"],
    queryFn: api.toolLatest,
    staleTime: 5 * 60_000,
    retry: 1,
  });
  const latestMap = useMemo(
    () => new Map((latestQ.data || []).map((l) => [l.app, l] as const)),
    [latestQ.data],
  );
  const [updating, setUpdating] = useState<string | null>(null);
  const [updMsg, setUpdMsg] = useState<{ app: string; ok: boolean; msg: string } | null>(null);
  const runUpdate = async (app: string) => {
    setUpdating(app);
    setUpdMsg(null);
    try {
      const msg = await api.toolUpdate(app);
      setUpdMsg({ app, ok: true, msg });
      void qc.invalidateQueries({ queryKey: ["tool_versions"] });
      void qc.invalidateQueries({ queryKey: ["tool_latest"] });
      void qc.invalidateQueries({ queryKey: ["tools"] });
    } catch (e) {
      setUpdMsg({ app, ok: false, msg: String(e) });
    } finally {
      setUpdating(null);
    }
  };
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <p className="min-w-0 truncate text-xs text-white/30">
          版本取自本機實際安裝；最新版查 registry（hermes 看自身輸出）；更新走官方通道，單次最長 10 分鐘
        </p>
        <button
          className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
          disabled={vers.isFetching || latestQ.isFetching}
          onClick={() => {
            void vers.refetch();
            void latestQ.refetch();
          }}
        >
          <Icon name="refresh" size={12} />
          {vers.isFetching || latestQ.isFetching ? "刷新中…" : "刷新"}
        </button>
      </div>
      {vers.isPending && <p className="text-sm text-white/30">讀取版本中…</p>}
      {vers.isError && (
        <p className="text-sm text-red-400">讀取失敗：{String(vers.error)}</p>
      )}
      {(vers.data || []).map((v) => {
        // 最新態：優先用 registry 比對，其次 hermes 自身輸出；都沒有=檢查中。
        const l = latestMap.get(v.app);
        const upToDate = l?.up_to_date ?? v.up_to_date ?? null;
        const latest = l?.latest ?? null;
        const showUpdate =
          v.installed && v.update_argv.length > 0 && upToDate !== true;
        return (
          <div
            key={v.app}
            className="flex items-center gap-2 rounded-xl border border-white/[0.06] p-3 text-sm"
          >
            <Logo name={v.app} size={17} className="shrink-0" />
            <span className="min-w-0 flex-1 truncate font-semibold text-white/85">
              {v.display}
            </span>
            {!v.installed ? (
              <span className="shrink-0 text-xs text-white/20">未安裝</span>
            ) : upToDate === true ? (
              <span className="flex shrink-0 items-center gap-1.5">
                <span className="font-mono text-xs text-[#30d158]">
                  {v.version ?? "未知版本"}
                </span>
                <span className="rounded-full bg-[#30d158]/10 px-2 py-0.5 text-[10px] font-medium text-[#30d158]">
                  已是最新
                </span>
              </span>
            ) : (
              <span className="shrink-0 font-mono text-xs text-white/45">
                {v.version ?? "未知版本"}
                {upToDate === false && latest != null && (
                  <span className="text-amber-300/90"> → {latest}</span>
                )}
                {upToDate === false && latest == null && v.app === "hermes" && (
                  <span className="text-amber-300/90">（有更新可用）</span>
                )}
                {upToDate == null && (
                  <span className="text-white/25">
                    {latestQ.isPending || latestQ.isFetching ? "（檢查中…）" : "（未查到最新版）"}
                  </span>
                )}
              </span>
            )}
            {showUpdate && (
              <button
                className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
                title={v.update_label ?? "更新"}
                disabled={updating === v.app}
                onClick={() => void runUpdate(v.app)}
              >
                <Icon name="download" size={12} />
                {updating === v.app ? "更新中…" : "更新"}
              </button>
            )}
          </div>
        );
      })}
      {updMsg && (
        <p
          className={`mt-1.5 text-xs break-all whitespace-pre-wrap ${
            updMsg.ok ? "text-white/45" : "text-red-400"
          }`}
        >
          {updMsg.msg}
        </p>
      )}
    </div>
  );
}

type ClaudeTierKey = "default" | "haiku" | "sonnet" | "opus" | "fable";
const CLAUDE_TIERS: { key: ClaudeTierKey; label: string }[] = [
  { key: "default", label: "預設模型" },
  { key: "haiku", label: "Haiku" },
  { key: "sonnet", label: "Sonnet" },
  { key: "opus", label: "Opus" },
  { key: "fable", label: "Fable" },
];

/** 單一 tier 儲存字串：「」= 留空；「manual:文本」= 手動輸入；其他 = 列表 actual_model。 */
function parseTierRaw(
  raw: string,
  options: { value: string }[],
): { sel: string; txt: string } {
  if (!raw) return { sel: "", txt: "" };
  if (raw.startsWith("manual:")) return { sel: "manual", txt: raw.slice(7) };
  if (options.some((o) => o.value === raw)) return { sel: raw, txt: "" };
  return { sel: "manual", txt: raw }; // 舊存檔任意字串 → 當手動文本
}

/** Claude 模型映射單檔位：下拉（來源模型＋手動）+ 有效值提示。 */
function ClaudeTierField(props: {
  label: string;
  options: { value: string; label: string; hint?: string }[];
  raw: string;
  followedBy: string;
  onRaw: (v: string) => void;
}) {
  const { sel, txt } = parseTierRaw(props.raw, props.options);
  const eff =
    sel === "manual"
      ? txt.trim() || props.followedBy
      : sel || props.followedBy;
  return (
    <div>
      <PopSelect
        label={props.label}
        value={sel}
        onChange={(v) =>
          props.onRaw(v === "manual" ? `manual:${txt}` : v)
        }
        options={[
          { value: "", label: "留空：跟隨上方選擇的模型" },
          ...props.options,
          { value: "manual", label: "手動輸入…" },
        ]}
      />
      {sel === "manual" && (
        <input
          className="field mt-1.5 w-full px-3.5 py-2 text-sm"
          placeholder="實際模型 ID（留空=跟隨上方）"
          value={txt}
          onChange={(e) => props.onRaw(`manual:${e.target.value}`)}
        />
      )}
      {eff && (
        <p className="mt-1 font-mono text-xs text-white/40">→ {eff}</p>
      )}
    </div>
  );
}

function SwitchDialog(props: {
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

const MODE_LABEL: Record<PricingMode, string> = {
  usage: "按量",
  subscription: "訂閱",
  tou: "峰谷",
};

function ChannelPricingForm(props: {
  providerId: number;
  initial: ProviderPricing | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [model, setModel] = useState(props.initial?.model_norm ?? "");
  const [mode, setMode] = useState<PricingMode>(
    props.initial?.mode ?? "usage",
  );
  const [rates, setRates] = useState({
    in_pm: props.initial?.in_pm ?? 0,
    out_pm: props.initial?.out_pm ?? 0,
    cache_read_pm: props.initial?.cache_read_pm ?? 0,
    cache_create_pm: props.initial?.cache_create_pm ?? 0,
  });
  const [subFee, setSubFee] = useState(props.initial?.sub_fee_usd ?? 0);
  const [subIncluded, setSubIncluded] = useState(
    props.initial?.sub_included_tokens ?? -1,
  );
  const [err, setErr] = useState("");

  const save = useMutation({
    mutationFn: () => {
      const input: ProviderPricingInput = {
        provider_id: props.providerId,
        model_norm: model.trim(),
        mode,
        in_pm: rates.in_pm,
        out_pm: rates.out_pm,
        cache_read_pm: rates.cache_read_pm,
        cache_create_pm: rates.cache_create_pm,
        sub_fee_usd: subFee,
        sub_included_tokens: subIncluded,
      };
      return api.ppUpsert(input);
    },
    onSuccess: props.onDone,
    onError: (e) => setErr(String(e)),
  });

  const inputCls =
    "field w-full px-3 py-1.5 text-xs";
  const num = (v: string) => Number(v) || 0;
  return (
    <div>
      <div className="mac-frow">
        <span className="mac-cap">模型 ID</span>
        <input
          className={inputCls}
          placeholder="* 表示該來源預設"
          value={model}
          onChange={(e) => setModel(e.target.value)}
        />
      </div>
      <div className="mac-frow">
        <span className="mac-cap">計費模式</span>
        <PopSelect
          value={mode}
          onChange={(v) => setMode(v as PricingMode)}
          options={[
            { value: "usage", label: "按量計費" },
            { value: "subscription", label: "訂閱制" },
            { value: "tou", label: "峰谷計費" },
          ]}
        />
      </div>
      {(
        [
          ["in_pm", "輸入/百萬"],
          ["out_pm", "輸出/百萬"],
          ["cache_read_pm", "快取讀/百萬"],
          ["cache_create_pm", "快取建/百萬"],
        ] as const
      ).map(([k, label]) => (
        <div key={k} className="mac-frow">
          <span className="mac-cap">
            {mode === "subscription" ? `超額${label}` : label}
          </span>
          <input
            className={inputCls}
            value={rates[k]}
            onChange={(e) =>
              setRates((r) => ({ ...r, [k]: num(e.target.value) }))
            }
          />
        </div>
      ))}
      {mode === "subscription" && (
        <>
          <div className="mac-frow">
            <span className="mac-cap">月費 $</span>
            <input
              className={inputCls}
              value={subFee}
              onChange={(e) => setSubFee(num(e.target.value))}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">免費額度</span>
            <input
              className={inputCls}
              placeholder="tokens，-1 無限"
              value={subIncluded}
              onChange={(e) => setSubIncluded(Number(e.target.value) || 0)}
            />
          </div>
        </>
      )}
      {mode === "tou" && (
        <div className="mt-2 rounded-xl border border-white/[0.06] bg-black/20 p-2.5">
          <div className="mb-1.5 text-[11px] font-medium text-white/45">
            峰谷時段（未命中用基礎價，可跨夜）
          </div>
          {model.trim() ? (
            <PeriodManager
              providerId={props.providerId}
              modelNorm={model.trim()}
            />
          ) : (
            <p className="text-[11px] text-amber-400">
              請先填寫上方模型 ID（`*` 表示該來源預設）
            </p>
          )}
        </div>
      )}
      {err && <p className="pt-1 text-xs text-red-400">{err}</p>}
      <div className="flex justify-end gap-2 pt-2.5">
        <button
          className="btn-ghost px-4 py-1.5 text-xs"
          onClick={props.onCancel}
        >
          取消
        </button>
        <button
          className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
          disabled={save.isPending}
          onClick={() => save.mutate()}
        >
          儲存
        </button>
      </div>
    </div>
  );
}

function PeriodManager(props: { providerId: number; modelNorm: string }) {
  const qc = useQueryClient();
  const list = useQuery({
    queryKey: ["periods", props.providerId, props.modelNorm],
    queryFn: () => api.ppPeriods(props.providerId, props.modelNorm),
  });
  const [editing, setEditing] = useState<PricingPeriod | "new" | null>(null);
  const [name, setName] = useState("");
  const [start, setStart] = useState("09:00");
  const [end, setEnd] = useState("18:00");
  const [ord, setOrd] = useState(0);
  const [rates, setRates] = useState({ in: 0, out: 0, cr: 0, cw: 0 });
  const [err, setErr] = useState("");

  const openNew = () => {
    setName("");
    setStart("09:00");
    setEnd("18:00");
    setOrd((list.data?.length ?? 0) * 10);
    setRates({ in: 0, out: 0, cr: 0, cw: 0 });
    setErr("");
    setEditing("new");
  };
  const openEdit = (p: PricingPeriod) => {
    setName(p.name);
    setStart(p.start);
    setEnd(p.end);
    setOrd(p.ord);
    setRates({ in: p.in_pm, out: p.out_pm, cr: p.cache_read_pm, cw: p.cache_create_pm });
    setErr("");
    setEditing(p);
  };

  const refresh = () => {
    void qc.invalidateQueries({
      queryKey: ["periods", props.providerId, props.modelNorm],
    });
    void qc.invalidateQueries({ queryKey: ["pp"] });
  };
  const { dialog: confirmDialog, ask } = useConfirm();
  const save = useMutation({
    mutationFn: async (): Promise<void> => {
      const input: PricingPeriodInput = {
        provider_id: props.providerId,
        model_norm: props.modelNorm,
        name: name.trim(),
        start: start.trim(),
        end: end.trim(),
        ord,
        in_pm: rates.in,
        out_pm: rates.out,
        cache_read_pm: rates.cr,
        cache_create_pm: rates.cw,
      };
      if (editing !== null && editing !== "new") {
        await api.ppPeriodUpdate(editing.id, input);
      } else {
        await api.ppPeriodAdd(input);
      }
    },
    onSuccess: () => {
      setEditing(null);
      refresh();
    },
    onError: (e) => setErr(String(e)),
  });
  const del = useMutation({
    mutationFn: (id: number) => api.ppPeriodDelete(id),
    onSuccess: refresh,
  });

  const inputCls =
    "field w-full px-3 py-1.5 text-xs";
  const num = (v: string) => Number(v) || 0;
  return (
    <div>
      <div className="mac-rows">
        {(list.data || []).map((p) => (
          <div key={p.id} className="mac-row !py-2">
            <div className="flex items-center gap-2 text-xs">
              <div className="min-w-0 flex-1 truncate text-white/70">
                {p.name}
                <span className="ml-1.5 font-mono text-white/35">
                  {p.start}-{p.end} · {p.in_pm}/{p.out_pm}
                </span>
              </div>
              <button
                className="shrink-0 rounded-full p-1 text-white/40 transition-colors hover:bg-white/10 hover:text-white"
                title="編輯時段"
                onClick={() => openEdit(p)}
              >
                <Icon name="pencil" size={12} />
              </button>
              <button
                className="shrink-0 rounded-full p-1 text-white/40 transition-colors hover:bg-red-500/10 hover:text-red-400"
                title="刪除時段"
                onClick={() => ask(`刪除時段 ${p.name}？`, () => del.mutate(p.id))}
              >
                <Icon name="trash" size={12} />
              </button>
            </div>
          </div>
        ))}
        {(list.data || []).length === 0 && !editing && (
          <div className="mac-row !py-2 text-[11px] text-white/20">
            尚無時段
          </div>
        )}
      </div>
      {editing ? (
        <div className="mac-rows mt-2 p-3">
          <div className="mac-frow">
            <span className="mac-cap">名稱</span>
            <input
              className={inputCls}
              placeholder="如 早峰"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">開始–結束</span>
            <div className="flex items-center gap-1.5">
              <input
                className={inputCls}
                placeholder="HH:MM"
                value={start}
                onChange={(e) => setStart(e.target.value)}
              />
              <span className="text-white/25">–</span>
              <input
                className={inputCls}
                placeholder="HH:MM"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
              />
            </div>
          </div>
          <div className="mac-frow">
            <span className="mac-cap">優先級</span>
            <input
              className={inputCls}
              value={ord}
              onChange={(e) => setOrd(Number(e.target.value) || 0)}
            />
          </div>
          {(
            [
              ["in", "輸入/百萬"],
              ["out", "輸出/百萬"],
              ["cr", "快取讀/百萬"],
              ["cw", "快取建/百萬"],
            ] as const
          ).map(([k, label]) => (
            <div key={k} className="mac-frow">
              <span className="mac-cap">{label}</span>
              <input
                className={inputCls}
                value={rates[k]}
                onChange={(e) =>
                  setRates((r) => ({ ...r, [k]: num(e.target.value) }))
                }
              />
            </div>
          ))}
          {err && <p className="pt-1 text-[11px] text-red-400">{err}</p>}
          <div className="flex justify-end gap-1.5 pt-2">
            <button
              className="btn-ghost px-3 py-1 text-xs"
              onClick={() => setEditing(null)}
            >
              取消
            </button>
            <button
              className="btn-primary px-3 py-1 text-xs"
              onClick={() => save.mutate()}
            >
              儲存
            </button>
          </div>
        </div>
      ) : (
        <button
          className="btn-ghost mt-2 flex items-center gap-1 px-2.5 py-1 text-xs"
          onClick={openNew}
        >
          <Icon name="plus" size={11} />
          時段
        </button>
      )}
      <p className="mt-1.5 text-[10px] text-white/15">
        重疊時取優先級（ord）最小命中的；`*` 模型時段為全來源回退。
      </p>
      {confirmDialog}
    </div>
  );
}

function fmtCatalogTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}

/** 模型行：唯讀摘要＋點選展開編輯（macOS inspector 手風琴）。 */
function ModelRowAcc(props: {
  row: UsableModel;
  catalogIds: string[];
  open: boolean;
  onToggle: () => void;
  onChanged: () => void;
  onMsg: (s: string) => void;
}) {
  const { row, catalogIds, open } = props;
  const [custom, setCustom] = useState(
    catalogIds.length > 0 && !catalogIds.includes(row.actual_model),
  );
  const [display, setDisplay] = useState(row.display_name);
  const [actual, setActual] = useState(row.actual_model);
  const [context, setContext] = useState(
    row.context_window != null ? String(row.context_window) : "",
  );
  const [reasoning, setReasoning] = useState<string>(
    row.reasoning,
  );
  // 展開時用最新值重置草稿
  useEffect(() => {
    if (open) {
      setDisplay(row.display_name);
      setActual(row.actual_model);
      setContext(row.context_window != null ? String(row.context_window) : "");
      setReasoning(row.reasoning);
      setCustom(catalogIds.length > 0 && !catalogIds.includes(row.actual_model));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open ]);

  const inputCls = "field w-full px-3 py-1.5 text-xs";

  const save = useMutation({
    mutationFn: (input: UsableModelInput) => api.modelUpdate(row.id, input),
    onSuccess: props.onChanged,
    onError: (e) => props.onMsg(String(e)),
  });
  const del = useMutation({
    mutationFn: () => api.modelDelete(row.id),
    onSuccess: props.onChanged,
    onError: (e) => props.onMsg(String(e)),
  });
  const { dialog: confirmDialog, ask } = useConfirm();
  const base: UsableModelInput = {
    provider_id: row.provider_id,
    display_name: row.display_name,
    actual_model: row.actual_model,
    context_window: row.context_window,
    reasoning: row.reasoning,
    ord: row.ord,
    enabled: row.enabled,
  };
  const commit = () => {
    const a = actual.trim();
    if (!a) {
      props.onMsg("實際請求模型不能為空");
      return;
    }
    save.mutate(
      {
        ...base,
        display_name: display.trim() || a,
        actual_model: a,
        context_window: context.trim() === "" ? null : Number(context) || 0,
        reasoning,
      },
      {
        // 表單儲存成功後自動收起（開關啟用走同一 mutation，不收起）
        onSuccess: () => {
          if (open) props.onToggle();
        },
      },
    );
  };

  return (
    <div
      className="mac-row clickable no-hover"
      data-open={open}
      onClick={props.onToggle}
    >
      <div className="flex items-center gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium text-white/85">
            {row.display_name}
          </div>
          <div className="mt-0.5 truncate font-mono text-[11px] text-white/30">
            {row.actual_model}
            {row.context_window ? ` · ctx ${row.context_window.toLocaleString()}` : ""}
            {row.reasoning && row.reasoning !== "unset" ? ` · ${row.reasoning.includes(",") ? row.reasoning.split(",").join(" / ") : row.reasoning}` : ""}
            {!row.enabled && <span className="ml-1.5 text-white/20">· 已停用</span>}
          </div>
        </div>
        <Toggle
          size="sm"
          checked={row.enabled}
          title="啟用"
          onChange={(v) => save.mutate({ ...base, enabled: v })}
        />
        <button
          className="shrink-0 rounded-full p-1.5 text-white/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="刪除"
          onClick={(e) => {
            e.stopPropagation();
            ask(`刪除映射 ${row.display_name}？`, () => del.mutate(), {
              message: "該模型的可用映射將被移除，不影響已抓取的目錄。",
            });
          }}
        >
          <Icon name="trash" size={13} />
        </button>
        <Icon
          name="chevron-down"
          size={12}
          className={`shrink-0 text-white/25 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </div>
      <div className="acc-body" data-open={open}>
        <div>
          <div
            className="mt-2.5 border-t border-white/[0.06] pt-1"
            onClick={(e) => e.stopPropagation()}
          >
          <div className="mac-frow">
            <span className="mac-cap">選單顯示名</span>
            <input
              className={inputCls}
              placeholder="空=同實際模型"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">實際請求模型</span>
            {custom ? (
              <div className="flex gap-1.5">
                <input
                  className={inputCls}
                  placeholder="模型 ID"
                  value={actual}
                  onChange={(e) => setActual(e.target.value)}
                />
                <button
                  className="btn-ghost flex shrink-0 items-center px-2 py-1 text-xs"
                  title="改用清單選擇"
                  onClick={() => setCustom(false)}
                >
                  <Icon name="chevron-left" size={12} />
                </button>
              </div>
            ) : (
              <PopSelect
                searchable
                value={
                  catalogIds.includes(actual) ? actual : "__custom"
                }
                onChange={(v) => {
                  if (v === "__custom") {
                    setCustom(true);
                    return;
                  }
                  setActual(v);
                }}
                options={[
                  ...catalogIds.map((id) => ({ value: id, label: id })),
                  { value: "__custom", label: "自訂輸入…" },
                ]}
              />
            )}
          </div>
          <div className="mac-frow">
            <span className="mac-cap">上下文視窗</span>
            <input
              className={inputCls}
              placeholder="例如 128000（空=不寫）"
              value={context}
              onChange={(e) => setContext(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">思考等級<span className="ml-1 font-normal text-white/25">空 = 全部檔位</span></span>
            <PopSelect
              multi
              value={reasoning}
              onChange={setReasoning}
              options={REASONING_OPTIONS.filter((o) => o.id !== "unset").map((o) => ({
                value: o.id,
                label: o.label,
              }))}
            />
          </div>
          <div className="flex gap-2 pt-2.5">
            <button
              className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
              disabled={save.isPending}
              onClick={commit}
            >
              儲存
            </button>
          </div>
          </div>
        </div>
      </div>
      {confirmDialog}
    </div>
  );
}

function ModelPanel(props: { provider: Provider; rowsClassName?: string; apiKeyOverride?: string; baseUrlOverride?: string }) {
  const qc = useQueryClient();
  const pid = props.provider.id;
  const catalog = useQuery({
    queryKey: ["catalog", pid],
    queryFn: () => api.catalogGet(pid),
    retry: false,
  });
  const rows = useQuery({
    queryKey: ["umodels", pid],
    queryFn: () => api.modelsList(pid),
  });
  const [adding, setAdding] = useState(false);
  const [actual, setActual] = useState("");
  const [display, setDisplay] = useState("");
  const [context, setContext] = useState("");
  const [reasoning, setReasoning] = useState<string>("");
  const [msg, setMsg] = useState("");

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["umodels", pid] });
    void qc.invalidateQueries({ queryKey: ["model_counts"] });
  };
  const fetch = useMutation({
    mutationFn: () => api.catalogFetch(pid, props.apiKeyOverride, props.baseUrlOverride),
    onSuccess: (c) => {
      setMsg(`已取得 ${c.models.length} 個模型`);
      void qc.invalidateQueries({ queryKey: ["catalog", pid] });
    },
    onError: (e) => setMsg(String(e)),
  });
  const add = useMutation({
    mutationFn: () => {
      const a = actual.trim();
      const input: UsableModelInput = {
        provider_id: pid,
        display_name: display.trim() || a,
        actual_model: a,
        context_window:
          context.trim() === "" ? null : Number(context) || 0,
        reasoning,
        ord: (rows.data?.length ?? 0) * 10,
        enabled: true,
      };
      return api.modelAdd(input);
    },
    onSuccess: () => {
      setAdding(false);
      setActual("");
      setDisplay("");
      setContext("");
      setReasoning("");
      setMsg("");
      refresh();
    },
    onError: (e) => setMsg(String(e)),
  });

  const catalogIds = (catalog.data?.models || []).map((m) => m.id);
  const [openId, setOpenId] = useState<number | null>(null);
  const inputClsSm = "field w-full px-3 py-1.5 text-xs";
  return (
    <div>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <button
          className="btn-ghost flex items-center gap-1.5 px-3.5 py-1.5 text-[13px] disabled:opacity-40"
          disabled={fetch.isPending}
          onClick={() => fetch.mutate()}
        >
          <Icon name="download" size={13} />
          取得模型清單
        </button>
        <button
          className="btn-ghost flex items-center gap-1.5 px-3.5 py-1.5 text-[13px]"
          onClick={() => {
            setActual(catalogIds[0] ?? "");
            setDisplay("");
            setContext("");
            setReasoning("");
            setOpenId(null);
            setAdding(true);
          }}
        >
          <Icon name="plus" size={12} />
          新增模型
        </button>
        <span className="ml-auto text-[11px] text-white/25">
          {catalog.data
            ? `共 ${catalog.data.models.length} 個 · 更新於 ${fmtCatalogTime(catalog.data.fetched_at)}`
            : "尚未抓取"}
        </span>
      </div>
      {!props.provider.api_key && !props.apiKeyOverride && (
        <p className="mb-2 text-xs text-amber-400">
          該來源尚未填寫 API Key（請在上方表單填入），否則抓取會 401/403
        </p>
      )}
      {msg && <p className="mb-2 text-xs text-amber-300">{msg}</p>}
      <div className={`mac-rows scroll-y ${props.rowsClassName ?? "max-h-[38vh]"}`}>
        {(rows.data || []).map((r) => (
          <ModelRowAcc
            key={r.id}
            row={r}
            catalogIds={catalogIds}
            open={openId === r.id}
            onToggle={() => setOpenId((o) => (o === r.id ? null : r.id))}
            onChanged={refresh}
            onMsg={setMsg}
          />
        ))}
        {(rows.data || []).length === 0 && (
          <div className="mac-row text-xs text-white/20">
            暫無使用模型，點「新增模型」建立（修改後需重啟對應 CLI 生效）。
          </div>
        )}
      </div>
      {adding && (
        <div className="mac-rows mt-2 p-3.5">
          <div className="mb-1 text-[13px] font-semibold text-white/80">
            新增模型
          </div>
          <div className="mac-frow">
            <span className="mac-cap">選單顯示名</span>
            <input
              className={inputClsSm}
              placeholder="空=同實際模型"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">實際請求模型</span>
            {catalogIds.length > 0 ? (
              <PopSelect
                searchable
                value={actual}
                onChange={setActual}
                options={catalogIds.map((id) => ({ value: id, label: id }))}
              />
            ) : (
              <input
                className={inputClsSm}
                placeholder="實際請求模型 ID"
                value={actual}
                onChange={(e) => setActual(e.target.value)}
              />
            )}
          </div>
          <div className="mac-frow">
            <span className="mac-cap">上下文視窗</span>
            <input
              className={inputClsSm}
              placeholder="空=不寫"
              value={context}
              onChange={(e) => setContext(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">思考等級<span className="ml-1 font-normal text-white/25">空 = 全部檔位</span></span>
            <PopSelect
              multi
              value={reasoning}
              onChange={setReasoning}
              options={REASONING_OPTIONS.filter((o) => o.id !== "unset").map((o) => ({
                value: o.id,
                label: o.label,
              }))}
            />
          </div>
          <div className="flex justify-end gap-2 pt-2.5">
            <button
              className="btn-ghost px-4 py-1.5 text-xs"
              onClick={() => setAdding(false)}
            >
              取消
            </button>
            <button
              className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
              disabled={add.isPending}
              onClick={() => add.mutate()}
            >
              儲存
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function ChannelPricing(props: {
  providers: Provider[];
  focusPid?: number | null;
}) {
  const qc = useQueryClient();
  const [pid, setPid] = useState<number | null>(props.focusPid ?? null);
  useEffect(() => {
    if (props.focusPid != null) setPid(props.focusPid);
  }, [props.focusPid]);
  const [editing, setEditing] = useState<ProviderPricing | "new" | null>(
    null,
  );
  const effectivePid = pid ?? props.providers[0]?.id ?? null;
  const rows = useQuery({
    queryKey: ["pp", effectivePid],
    queryFn: () => api.ppList(effectivePid),
    enabled: effectivePid != null,
  });

  const refresh = () =>
    void qc.invalidateQueries({ queryKey: ["pp", effectivePid] });
  const del = useMutation({
    mutationFn: ({ p, m }: { p: number; m: string }) => api.ppDelete(p, m),
    onSuccess: refresh,
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <PopSelect
          className="min-w-[160px] flex-1"
          value={effectivePid != null ? String(effectivePid) : ""}
          onChange={(v) => setPid(Number(v))}
          options={props.providers.map((p) => ({
            value: String(p.id),
            label: p.name,
            hint: p.base_url,
          }))}
        />
        <button
          className="btn-ghost flex shrink-0 items-center gap-1.5 px-3.5 py-1.5 text-[13px]"
          onClick={() => {
            setEditing("new");
          }}
        >
          <Icon name="plus" size={12} />
          模型定價
        </button>
      </div>
      <div className="mac-rows scroll-y max-h-[38vh]">
        {(rows.data || []).map((r) => {
          const key = `${r.provider_id}:${r.model_norm}`;
          const open =
            editing !== null &&
            editing !== "new" &&
            `${editing.provider_id}:${editing.model_norm}` === key;
          return (
            <div
              key={key}
              className="mac-row clickable"
              data-open={open}
              onClick={() =>
                setEditing((e) =>
                  e !== null &&
                  e !== "new" &&
                  `${e.provider_id}:${e.model_norm}` === key
                    ? null
                    : r,
                )
              }
            >
              <div className="flex items-center gap-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate font-mono text-[13px] text-white/85">
                    {r.model_norm === "*" ? "*（該來源預設）" : r.model_norm}
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-white/30">
                    基礎 {r.in_pm}/{r.out_pm}
                    {r.mode === "subscription" &&
                      ` · 月費 $${r.sub_fee_usd} · 已用 ${r.used_this_month.toLocaleString()}`}
                    {r.mode === "tou" &&
                      ` · 峰谷 ${r.periods.length} 時段`}
                  </div>
                  {r.mode === "subscription" &&
                    r.sub_included_tokens >= 0 && (
                      <div className="mt-1.5 h-1 w-24 overflow-hidden rounded-full bg-white/[0.06]">
                        <div
                          className="h-full rounded-full bg-(--mac-accent)/50 bar-anim"
                          style={{
                            width: `${Math.min(100, (r.used_this_month / Math.max(1, r.sub_included_tokens)) * 100)}%`,
                          }}
                        />
                      </div>
                    )}
                </div>
                <span className="shrink-0 rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-medium text-white/40">
                  {MODE_LABEL[r.mode]}
                </span>
                <button
                  className="shrink-0 rounded-full p-1.5 text-white/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
                  title="刪除"
                  onClick={(e) => {
                    e.stopPropagation();
                    ask(`刪除 ${r.model_norm} 的來源定價？`, () =>
                      del.mutate({ p: r.provider_id, m: r.model_norm }),
                    );
                  }}
                >
                  <Icon name="trash" size={13} />
                </button>
                <Icon
                  name="chevron-down"
                  size={12}
                  className={`shrink-0 text-white/25 transition-transform ${open ? "rotate-180" : ""}`}
                />
              </div>
              <div className="acc-body" data-open={open}>
            <div>
                  <div
                    className="mt-2.5 border-t border-white/[0.06] pt-1"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <ChannelPricingForm
                      key={key}
                      providerId={r.provider_id}
                      initial={editing !== null && editing !== "new" ? editing : null}
                      onDone={() => {
                        setEditing(null);
                        refresh();
                      }}
                      onCancel={() => setEditing(null)}
                    />
                  </div>
                </div>
              </div>
            </div>
          );
        })}
        {(rows.data || []).length === 0 && (
          <div className="mac-row text-xs text-white/20">
            該來源還沒有單獨定價，會回退到全域預設定價（見下方）。
          </div>
        )}
      </div>
      {editing === "new" && effectivePid != null && (
        <div className="mac-rows mt-2 p-3.5">
          <div className="mb-1 text-[13px] font-semibold text-white/80">
            新增模型定價
          </div>
          <ChannelPricingForm
            providerId={effectivePid}
            initial={null}
            onDone={() => {
              setEditing(null);
              refresh();
            }}
            onCancel={() => setEditing(null)}
          />
        </div>
      )}
      {confirmDialog}
    </div>
  );
}

export default function ProvidersPage() {
  const qc = useQueryClient();
  const port = useGatewayPort();
  const providers = useQuery({
    queryKey: ["providers"],
    queryFn: api.providersList,
  });
  const tools = useQuery({
    queryKey: ["tools", port],
    queryFn: () => api.toolsDetect(port),
    refetchInterval: 10000,
  });
  const [editing, setEditing] = useState<Provider | "new" | null>(null);
  const [gpCount, setGpCount] = useState<number | null>(null);
  const [gpOpen, setGpOpen] = useState(false);
  const [view, setView] = useState<"sources" | "tools">("sources");
  const [switching, setSwitching] = useState<ToolStatus | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [detailTab, setDetailTab] = useState<"models" | "pricing">("models");
  const counts = useQuery({
    queryKey: ["model_counts"],
    queryFn: api.modelCounts,
  });
  const countMap = useMemo(
    () => new Map((counts.data || []).map(([pid, n]) => [pid, n] as const)),
    [counts.data],
  );
  const selId = selected ?? providers.data?.[0]?.id ?? null;
  const sel = (providers.data || []).find((p) => p.id === selId) ?? null;

  const del = useMutation({
    mutationFn: (id: number) => api.providerDelete(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["providers"] }),
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  const [toggling, setToggling] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<{ app: string; msg: string } | null>(null);
  const [rowMsg, setRowMsg] = useState<{ app: string; msg: string } | null>(null);
  const [doctorLines, setDoctorLines] = useState<string[] | null>(null);
  const [doctorBusy, setDoctorBusy] = useState(false);
  // 來源拖拽排序（零 DOM 位移：開拖快照容器內座標，殘影 transform 跟手，
  // 藍線絕對定位；window 級監聽保證任何鬆手位置/手柄狀態都不丟事件）。
  const [dragId, setDragId] = useState<number | null>(null);
  const [drop, setDropState] = useState<{ idx: number; y: number } | null>(null);
  const [reordering, setReordering] = useState(false);
  const [listErr, setListErr] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{
    id: number;
    startY: number;
    lastY: number;
    moved: boolean;
    cardEl: HTMLElement | null;
  } | null>(null);
  const boundsRef = useRef<Array<{ top: number; bottom: number }> | null>(null);
  const idsRef = useRef<number[]>([]);
  const dropRef = useRef<{ idx: number; y: number } | null>(null);
  const suppressClick = useRef(false);
  const rafRef = useRef<number | null>(null);
  const detachWinRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const clearSuppress = () => {
      suppressClick.current = false;
    };
    window.addEventListener("pointerdown", clearSuppress, true);
    return () => {
      window.removeEventListener("pointerdown", clearSuppress, true);
      detachWinRef.current?.();
    };
  }, []);

  /** 提交全排列（拖拽落點 / 未來鍵盤排序共用）。 */
  const commitOrder = async (ids: number[]) => {
    setReordering(true);
    setListErr(null);
    try {
      await api.providerReorder(ids);
      void qc.invalidateQueries({ queryKey: ["providers"] });
    } catch (e) {
      setListErr(String(e));
      void qc.invalidateQueries({ queryKey: ["providers"] }); // 失敗回退到庫內真序
    } finally {
      setReordering(false);
    }
  };

  const setDrop = (v: { idx: number; y: number } | null) => {
    dropRef.current = v;
    setDropState(v);
  };

  /** 以開拖快照的容器內座標計算插入位與藍線 Y（拖拽期間版面零位移，快照全程有效）。 */
  const computeDrop = (): { idx: number; y: number } | null => {
    const d = dragRef.current;
    const box = listRef.current;
    const bounds = boundsRef.current;
    if (!d || !box || !bounds || bounds.length < 2) return null;
    const k = idsRef.current.indexOf(d.id);
    if (k < 0) return null;
    const y = d.lastY - box.getBoundingClientRect().top;
    let idx = 0;
    const rest: number[] = [];
    for (let i = 0; i < bounds.length; i++) {
      if (i === k) continue;
      rest.push(i);
      if (y > (bounds[i].top + bounds[i].bottom) / 2) idx += 1;
    }
    let lineY: number;
    if (idx === 0) lineY = bounds[rest[0]].top - 4;
    else if (idx >= rest.length) lineY = bounds[rest[rest.length - 1]].bottom + 4;
    else lineY = (bounds[rest[idx - 1]].bottom + bounds[rest[idx]].top) / 2;
    return { idx, y: lineY };
  };

  /** 拖拽唯一出口：拆 window 監聽、取消 rAF、清殘影 transform。 */
  const teardownDrag = () => {
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    detachWinRef.current?.();
    const d = dragRef.current;
    if (d?.cardEl) d.cardEl.style.transform = "";
    dragRef.current = null;
    boundsRef.current = null;
    idsRef.current = [];
  };

  /** rAF 節流：殘影跟手（imperative transform，不進 React）+ 落點僅在變化時 setState。 */
  const scheduleDragFrame = () => {
    if (rafRef.current != null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const d = dragRef.current;
      if (!d || !d.moved) return;
      if (d.cardEl) d.cardEl.style.transform = `translateY(${d.lastY - d.startY}px)`;
      const v = computeDrop();
      if (v && dropRef.current?.idx !== v.idx) setDrop(v);
    });
  };

  const onWinMove = (e: PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    d.lastY = e.clientY;
    if (!d.moved) {
      if (Math.abs(e.clientY - d.startY) <= 4) return;
      d.moved = true;
    }
    scheduleDragFrame();
  };

  const onWinCancel = () => {
    teardownDrag();
    setDragId(null);
    setDrop(null);
  };

  const onWinKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") onWinCancel();
  };

  const onWinUp = (e: PointerEvent) => {
    const d = dragRef.current;
    if (!d) {
      teardownDrag();
      return;
    }
    d.lastY = e.clientY;
    const final = d.moved ? computeDrop() : null;
    teardownDrag();
    setDragId(null);
    setDrop(null);
    if (!d.moved || !final) return;
    suppressClick.current = true;
    const list = providers.data || [];
    const rest = list.map((p) => p.id).filter((x) => x !== d.id);
    const clamped = Math.max(0, Math.min(final.idx, rest.length));
    rest.splice(clamped, 0, d.id);
    const orig = list.map((p) => p.id);
    if (rest.join(",") === orig.join(",")) return; // 原地，不提交
    void commitOrder(rest);
  };

  /** 手柄按下：快照容器內座標、掛 window 監聽，開始一次潛在拖拽（未移動則視為普通點擊）。 */
  const gripDown = (e: RPointerEvent<HTMLSpanElement>, id: number) => {
    if (reordering) return;
    e.preventDefault();
    const box = listRef.current;
    if (!box) return;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* 舊 webview 無 capture 也能跑，window 監聽兜底 */
    }
    const bounds: Array<{ top: number; bottom: number }> = [];
    const ids: number[] = [];
    for (const child of Array.from(box.children) as HTMLElement[]) {
      const pid = Number(child.dataset.pid);
      if (!pid) continue;
      const top = child.offsetTop;
      bounds.push({ top, bottom: top + child.offsetHeight });
      ids.push(pid);
    }
    boundsRef.current = bounds;
    idsRef.current = ids;
    dragRef.current = {
      id,
      startY: e.clientY,
      lastY: e.clientY,
      moved: false,
      cardEl: box.querySelector<HTMLElement>(`[data-pid="${id}"]`),
    };
    const detach = () => {
      window.removeEventListener("pointermove", onWinMove);
      window.removeEventListener("pointerup", onWinUp);
      window.removeEventListener("pointercancel", onWinCancel);
      window.removeEventListener("keydown", onWinKey);
      detachWinRef.current = null;
    };
    detachWinRef.current = detach;
    window.addEventListener("pointermove", onWinMove);
    window.addEventListener("pointerup", onWinUp);
    window.addEventListener("pointercancel", onWinCancel);
    window.addEventListener("keydown", onWinKey);
    setDragId(id);
    setDrop(null);
    setListErr(null);
  };

  const runDoctor = () => {
    setDoctorBusy(true);
    api
      .codexDoctor(port)
      .then((lines) => setDoctorLines(lines))
      .catch((e) => setDoctorLines([`❌ 體檢執行失敗：${String(e)}`]))
      .finally(() => setDoctorBusy(false));
  };

  const refreshTools = () => {
    void qc.invalidateQueries({ queryKey: ["tools", port] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
  };

  /** 行開關（只開/關，永不彈框）：關=還原接管前備份；開=按上次來源/模型強制經網關接管。詳情設定一律點行左側進入。 */
  const toggleRow = (t: ToolStatus, on: boolean) => {
    setRowErr(null);
    setRowMsg(null);
    if (!on) {
      setToggling(t.app);
      api
        .switchRestore(t.app, port)
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

  /** 開關 ON：上次參數可用則沿用來源/模型/Key 偏好，但路由強制為本網關。 */
  const toggleOn = async (t: ToolStatus) => {
    const saved = loadLastSwitch(t.app);
    const savedKey = saved?.api_key ?? "";
    if (saved) {
      const req: SwitchRequest = {
        ...saved,
        app: t.app,
        via_gateway: true,
        base_url: gatewayUrl(port, t.app),
        api_key: savedKey,
        gen_catalog: saved.gen_catalog ?? true,
        catalog_union: saved.catalog_union ?? false,
        direct_upstream: !savedKey.trim(),
      };
      await doApply(t, req, "已接管", req.direct_upstream);
      return;
    }
    const provider = (providers.data || []).find((p) => p.id === selId);
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
      (tools.data || []).find((x) => x.app === t.app)?.current_model ?? null;
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
        base_url: gatewayUrl(port, t.app),
        api_key: apiKey,
        model: row.actual_model,
        via_gateway: true,
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

  /** 執行接管寫入（開關/自動共用）：存檔＋行內成功信息＋刷新；失敗只報行內錯誤。 */
  const doApply = async (
    t: ToolStatus,
    req: SwitchRequest,
    okPrefix: string,
    upstreamKeyWritten: boolean,
  ) => {
    setToggling(t.app);
    try {
      await api.switchApply(req, port);
      saveLastSwitch(t.app, req);
      const pname =
        (providers.data || []).find((p) => p.id === req.provider_id)?.name ?? "";
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
   * 來源切換自動重接管共用入口（按當前所選來源組裝，永不彈框）。
   * 模型解析：上次模型（或現行模型）仍在該來源啟用表則沿用，否則取該來源首個啟用模型；
   * Key/目錄/推理偏好沿用上次設定。來源無啟用模型時報行內錯誤，不寫入。
   */
  const applyCurrentSelection = async (t: ToolStatus) => {
    const provider = (providers.data || []).find((p) => p.id === selId);
    if (!provider) {
      setRowErr({ app: t.app, msg: "請先選擇一個上游來源" });
      return;
    }
    try {
      const { req, upstreamKeyWritten } = await resolveSelectionReq(t, provider);
      await doApply(t, req, "已按新來源重接管", upstreamKeyWritten);
    } catch (e) {
      setRowErr({ app: t.app, msg: e instanceof Error ? e.message : String(e) });
    }
  };

  // 來源切換自動重接管：所選來源變化且 Codex 正被接管時，按新來源直接重接管
  //（模型沿用現行/上次，否則新來源首個啟用模型；Codex 重啟後選擇器同步）。
  const prevSelId = useRef<number | null>(null);
  useEffect(() => {
    if (prevSelId.current === null) {
      prevSelId.current = selId;
      return;
    }
    if (prevSelId.current === selId) return;
    prevSelId.current = selId;
    if (selId == null) return;
    const codex = (tools.data || []).find((x) => x.app === "codex");
    if (!codex?.gateway_active || toggling === "codex") return;
    void applyCurrentSelection(codex);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selId, tools.data, toggling]);

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["providers"] });
    void qc.invalidateQueries({ queryKey: ["tools", port] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
    void qc.invalidateQueries({ queryKey: ["model_counts"] });
  };

  const activeTools = (tools.data || []).filter((t) => t.gateway_active).length;

  // ---- 來源卡片（指針拖拽：手柄 pointerdown/move/up，接管 pointer capture） ----
  const srcCard = (p: Provider, i: number) => (
    <div
      key={p.id}
      data-pid={p.id}
      onClick={() => {
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        setSelected(p.id);
      }}
      title="點擊選中，在下方查看模型與定價；按住左側手柄拖拽排序"
      style={{
        animationDelay: `${Math.min(i * 50, 300)}ms`,
        ...(dragId === p.id
          ? { position: "relative", zIndex: 10, transition: "none" }
          : {}),
      }}
      data-active={p.id === selId}
      className={`sel-card anim-rise cursor-pointer rounded-xl border p-3.5 text-sm transition-all ${
        p.id === selId
          ? ""
          : "border-white/[0.06] hover:border-white/10"
      } ${dragId === p.id ? "opacity-40" : ""}`}
    >
      <div className="flex items-center gap-2">
        <span
          title="按住拖拽排序"
          onPointerDown={(e) => gripDown(e, p.id)}
          style={{ touchAction: "none" }}
          className="grid shrink-0 cursor-grab grid-cols-2 gap-x-[3px] gap-y-[3px] px-1 text-white/20 select-none active:cursor-grabbing"
        >
          {[0, 1, 2, 3, 4, 5].map((d) => (
            <span key={d} className="h-[3px] w-[3px] rounded-full bg-current" />
          ))}
        </span>
        <span className="min-w-0 flex-1 truncate font-semibold text-white/85">
          {p.name}
          <span className="ml-2 rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-medium text-white/30">
            {countMap.get(p.id) ?? 0} 模型
          </span>
        </span>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-white/10 hover:text-white"
          title="編輯"
          onClick={(e) => {
            e.stopPropagation();
            setSelected(p.id);
            setEditing(p);
          }}
        >
          <Icon name="pencil" size={13} />
        </button>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="刪除"
          onClick={(e) => {
            e.stopPropagation();
            ask(`刪除來源 ${p.name}？`, () => del.mutate(p.id), {
              message: "該來源的使用模型與定價將一併移除，已分發的 Key 會失效。",
            });
          }}
        >
          <Icon name="trash" size={13} />
        </button>
      </div>
      <p className="mt-1.5 truncate font-mono text-xs text-white/30">
        {p.base_url}
      </p>
      <div className="mt-0.5 flex items-center gap-2 text-xs">
        <p className="min-w-0 flex-1 truncate text-white/20">
          {(p.models || []).join(", ") || "未登記模型"}
        </p>
        <span className="shrink-0 text-[10px] text-white/25">
          {p.app_type} · {p.api_format} ·{" "}
          {p.auth_scheme === "goog-key" ? "Google Key" : "Bearer"}
        </span>
      </div>
    </div>
  );

  const srcList = providers.data || [];

  return (
    <div className="space-y-4">
      <div className="glass p-5">
        <div className="mb-3">
          <Segmented
            className="w-full"
            btnClassName="px-3 py-3 text-sm font-medium"
            value={view}
            onChange={(v) => setView(v as "sources" | "tools")}
            options={[
              { id: "sources", label: `來源（${providers.data?.length ?? 0}）` },
              {
                id: "tools",
                label: `本機工具（${activeTools}/${tools.data?.length ?? 0} 接管中）`,
              },
            ]}
          />
        </div>
        {view === "sources" && (
          <div className="grid items-start gap-4 lg:grid-cols-2">
            <div className="min-w-0">
            <button
              className="btn-primary mb-3 flex items-center gap-1.5 px-4 py-1.5 text-sm"
              onClick={() => setEditing("new")}
            >
              <Icon name="plus" size={13} />
              添加來源
            </button>
            {providers.isPending && (
              <p className="text-sm text-white/30">載入中…</p>
            )}
        {listErr && (
          <p className="mb-2 break-all text-xs text-red-400">{listErr}</p>
        )}
        <div ref={listRef} className="relative space-y-2">
          {srcList.map((p, i) => srcCard(p, i))}
          {dragId != null && drop != null && (
            <div
              key="drop-line"
              className="pointer-events-none absolute right-0 left-0 h-1 rounded-full bg-[#0A84FF]"
              style={{ top: drop.y }}
            />
          )}
        </div>
            </div>
            <div className="min-w-0 lg:sticky lg:top-0">
              {sel && (
                <div key={sel.id} className="glass anim-rise p-5">
                  <div className="mb-4 flex flex-wrap items-center gap-2.5">
                    <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
                      <Icon name="server" size={18} />
                    </span>
                    <div className="min-w-0 flex-1 basis-40">
                      <div className="truncate text-[15px] font-semibold tracking-tight text-white">
                        {sel.name}
                      </div>
                      <div className="truncate font-mono text-[11px] text-white/30">
                        {sel.base_url}
                      </div>
                    </div>
                    <div className="hidden flex-1 sm:block" />
                    <Segmented
                      value={detailTab}
                      onChange={(v) => setDetailTab(v as "models" | "pricing")}
                      options={[
                        { id: "models", label: "模型" },
                        { id: "pricing", label: "定價" },
                      ]}
                      btnClassName="px-3.5 py-1 text-[13px] font-medium"
                    />
                  </div>
                  {detailTab === "models" ? (
                    <>
                      <div className="mb-2 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                        <span>使用模型</span>
                      </div>
                      <ModelPanel key={sel.id} provider={sel} />
                    </>
                  ) : (
                    <ChannelPricing
                      key={`pp-${sel.id}`}
                      providers={providers.data || []}
                      focusPid={sel.id}
                    />
                  )}
                </div>
              )}
            </div>
          </div>
        )}
        {view === "tools" && (
          <>
        {tools.isPending && <p className="text-sm text-white/30">檢測中…</p>}
        {tools.isError && (
          <p className="text-sm text-red-400">檢測失敗：{String(tools.error)}</p>
        )}
        <div className="space-y-2">
          {(tools.data || []).map((t, i) => (
            <div
              key={t.app}
              style={{ animationDelay: `${Math.min(i * 50, 300)}ms` }}
              onClick={() => setSwitching(t)}
              title="點擊進入切換設定"
              className="glass-float hover-line anim-rise cursor-pointer rounded-xl border p-3.5 text-sm"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-2 font-semibold text-white/85">
                  <Logo name={t.app} size={17} className="shrink-0" />
                  <span className="truncate">{t.display}</span>
                  {!t.installed && (
                    <span className="shrink-0 text-xs text-white/20">未安裝</span>
                  )}
                  {t.gateway_active && (
                    <span className="shrink-0 rounded-full bg-[#30d158]/10 px-2.5 py-0.5 text-[10px] font-medium text-[#30d158]">
                      網關接管中
                    </span>
                  )}
                </span>
                <Toggle
                  size="sm"
                  checked={t.gateway_active}
                  disabled={!t.supported_switch || toggling === t.app}
                  title={
                    t.supported_switch
                      ? t.gateway_active
                        ? "關閉：還原接管前備份"
                        : "開啟：直接接管（詳情點行左側）"
                      : "僅檢測，自動切換待支援"
                  }
                  onChange={(v) => toggleRow(t, v)}
                />
              </div>
              {t.gateway_active && (
                <>
                  <p className="mt-1.5 truncate font-mono text-xs text-white/30">
                    {t.current_base_url || "（未檢測到 base_url）"}
                  </p>
                  <p className="mt-0.5 truncate text-xs text-white/20">
                    模型：{t.current_model || "未知"}
                  </p>
                </>
              )}
              {rowErr?.app === t.app && (
                <p className="mt-1.5 break-all text-xs text-red-400">{rowErr.msg}</p>
              )}
              {rowMsg?.app === t.app && (
                <p className="mt-1.5 break-all text-xs text-white/45">{rowMsg.msg}</p>
              )}
              {t.app === "codex" && (
                <div className="mt-1.5">
                  <button
                    className="rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white"
                    disabled={doctorBusy}
                    onClick={() => runDoctor()}
                  >
                    {doctorBusy ? "體檢中…" : "接管前體檢"}
                  </button>
                  {doctorLines && (
                    <ul className="mt-1.5 space-y-0.5 font-mono text-[11px] leading-relaxed">
                      {doctorLines.map((l, i) => (
                        <li
                          key={i}
                          className={
                            l.startsWith("❌")
                              ? "text-red-400"
                              : l.startsWith("⚠️")
                                ? "text-amber-300/90"
                                : l.startsWith("ℹ️")
                                  ? "text-white/50"
                                  : "text-[#30d158]/80"
                          }
                        >
                          {l}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
        <p className="mt-3 text-xs text-white/15">
          Cursor / Antigravity / DSH 僅檢測不寫入設定（Cursor 設定在 SQLite、
          Antigravity 格式未定、DSH endpoint 由 profile bundle 決定）。
          這些工具請手動把 base_url 指向本網關，並在請求頭帶 x-tg-app 標識。
        </p>
          </>
        )}
      </div>
      {switching && (
        <SwitchDialog
          tool={switching}
          providers={providers.data || []}
          port={port}
          onClose={() => setSwitching(null)}
          onApplied={refresh}
          initialProviderId={selId}
        />
      )}
      {editing && (
        <ProviderSheet
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            refresh();
          }}
        />
      )}
      <div className="glass px-5 py-4">
        <button
          onClick={() => setGpOpen((o) => !o)}
          className="flex w-full items-center gap-2 text-left"
        >
          <Icon
            name="chevron-right"
            size={13}
            className={`shrink-0 text-white/25 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] ${gpOpen ? "rotate-90" : ""}`}
          />
          <span className="text-sm font-semibold tracking-tight text-white/80">
            {view === "tools" ? "本機工具版本" : "全域預設定價"}
          </span>
          {view !== "tools" && gpCount !== null && (
            <span className="rounded-full bg-white/[0.07] px-2 py-px text-[10px] font-medium text-white/45">
              {gpCount}
            </span>
          )}
          <span className="truncate text-xs text-white/30">
            {view === "tools"
              ? "已安裝工具的實際版本與一鍵更新"
              : "來源未單獨定價時的回退"}
          </span>
        </button>
        <div className="acc-body" data-open={gpOpen}>
          <div>
            <div className="mt-3 border-t border-white/[0.06] pt-3">
              {view === "tools" ? (
                <ToolVersions />
              ) : (
                <GlobalPricingManager onCount={setGpCount} />
              )}
            </div>
          </div>
        </div>
      </div>
      {confirmDialog}
    </div>
  );
}
