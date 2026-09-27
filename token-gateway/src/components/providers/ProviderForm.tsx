//! 由 `Providers.tsx` 拆分而來（原檔 2,845 行）。程式碼語意未改，只搬位置。

import {
  useMemo,
  useState,
} from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  api,
  type Provider,
  type ProviderInput,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  PopSelect,
} from "../PopSelect";
import {
  Logo,
} from "../logos";
import {
  ModelPanel,
} from "./ModelCatalog";
import {
  API_FORMATS,
  APP_TYPES,
  AUTH_SCHEMES,
  EMPTY_INPUT,
} from "./providersTypes";

export function ProviderForm(props: {
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
export function ProviderSheet(props: {
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

export function ToolVersions() {
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
          版本取自本機實際安裝；最新版查 registry；更新走官方通道，單次最長 10 分鐘
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
        // 最新態：用 registry 比對；比不到＝檢查中。
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

export type ClaudeTierKey = "default" | "haiku" | "sonnet" | "opus" | "fable";
export const CLAUDE_TIERS: { key: ClaudeTierKey; label: string }[] = [
  { key: "default", label: "預設模型" },
  { key: "haiku", label: "Haiku" },
  { key: "sonnet", label: "Sonnet" },
  { key: "opus", label: "Opus" },
  { key: "fable", label: "Fable" },
];

/** 單一 tier 儲存字串：「」= 留空；「manual:文本」= 手動輸入；其他 = 列表 actual_model。 */
export function parseTierRaw(
  raw: string,
  options: { value: string }[],
): { sel: string; txt: string } {
  if (!raw) return { sel: "", txt: "" };
  if (raw.startsWith("manual:")) return { sel: "manual", txt: raw.slice(7) };
  if (options.some((o) => o.value === raw)) return { sel: raw, txt: "" };
  return { sel: "manual", txt: raw }; // 舊存檔任意字串 → 當手動文本
}

/** Claude 模型映射單檔位：下拉（來源模型＋手動）+ 有效值提示。 */
export function ClaudeTierField(props: {
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
