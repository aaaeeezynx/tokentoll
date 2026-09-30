//! 由 `components/providers/ProviderForm.tsx` 拆分而來。程式碼語意未改，只搬位置。

import {
  useState,
} from "react";
import {
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  api,
  type Preset,
  type Provider,
  type ProviderInput,
} from "../../../lib/api";
import {
  Icon,
} from "../../icons";
import {
  PopSelect,
} from "../../PopSelect";
import {
  ModelPanel,
} from "../ModelCatalog";
import {
  API_FORMATS,
  APP_TYPES,
  AUTH_SCHEMES,
  EMPTY_INPUT,
} from "../providersTypes";

export function ProviderForm(props: {
  initial: Provider | null;
  /** 新建時可從預設開始（只填連線資訊，金鑰與模型仍由使用者處理）。 */
  preset?: Preset | null;
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
      : props.preset
        ? {
            ...EMPTY_INPUT,
            name: props.preset.name,
            app_type: props.preset.app_type || "universal",
            api_format: props.preset.api_format,
            base_url: props.preset.base_url,
            auth_scheme: props.preset.auth_scheme || "bearer",
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
