import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import {
  api,
  type KeyInput,
  type LocalKey,
  type Provider,
} from "../../lib/api";
import { PopSelect } from "../PopSelect";
import { Toggle } from "../Toggle";
import { APP_META, Logo } from "../logos";
import { ModelWhitelist } from "./ModelWhitelist";

export function KeyForm(props: {
  providers: Provider[];
  initial: LocalKey | null;
  onDone: () => void;
  onCancel: () => void;
  onCreatedSecret: (secret: string) => void;
}) {
  const [name, setName] = useState(props.initial?.name ?? "");
  const [providerId, setProviderId] = useState<string>(
    props.initial?.provider_id != null ? String(props.initial.provider_id) : "",
  );
  const [quota, setQuota] = useState(
    props.initial ? String(props.initial.quota_tokens) : "-1",
  );
  const [qpm, setQpm] = useState(
    props.initial ? String(props.initial.rate_limit_qpm) : "60",
  );
  const [models, setModels] = useState<string[]>(
    props.initial?.allowed_models || [],
  );
  const [apps, setApps] = useState<string[]>(
    props.initial?.allowed_apps || [],
  );
  const [err, setErr] = useState("");

  const save = useMutation({
    mutationFn: () => {
      const input: KeyInput = {
        name: name.trim(),
        provider_id: providerId === "" ? null : Number(providerId),
        quota_tokens: Number(quota) || -1,
        rate_limit_qpm: Number(qpm) || 0,
        allowed_models: models,
        allowed_apps: apps,
        expires_at: props.initial?.expires_at ?? null,
      };
      return props.initial
        ? api.keyUpdate(props.initial.id, input).then(() => null)
        : api.keyCreate(input);
    },
    onSuccess: (created) => {
      if (created) props.onCreatedSecret(created.secret);
      props.onDone();
    },
    onError: (e) => setErr(String(e)),
  });

  const fieldCls = "field w-full px-3 py-1.5 text-[13px]";

  return (
    <div className="space-y-4">
      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
          基本資訊
        </div>
        <div className="mac-frow">
          <span className="mac-cap">名稱</span>
          <input
            className={fieldCls}
            placeholder="如 codex-主力"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">綁定上游</span>
          <PopSelect
            value={providerId}
            onChange={setProviderId}
            options={[
              { value: "", label: "選擇綁定的上游來源…" },
              ...props.providers.map((p) => ({
                value: String(p.id),
                label: p.name,
                hint: p.base_url,
              })),
            ]}
          />
        </div>
      </div>

      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
          用量限制
        </div>
        <div className="mac-frow">
          <span className="mac-cap">配額 tokens</span>
          <input
            className={fieldCls}
            placeholder="-1 不限"
            value={quota}
            onChange={(e) => setQuota(e.target.value)}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">限流 次/分</span>
          <input
            className={fieldCls}
            placeholder="0 不限"
            value={qpm}
            onChange={(e) => setQpm(e.target.value)}
          />
        </div>
      </div>

      <div className="form-section">
        <div className="mb-1 flex items-center gap-2">
          <span className="text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
            模型白名單
          </span>
          {models.length > 0 && (
            <span className="ml-auto rounded-full bg-fg/[0.08] px-2 py-0.5 text-[10px] font-medium text-fg/50">
              {models.length}
            </span>
          )}
        </div>
        <p className="text-[11px] text-fg/30">空＝全部允許；選了的只有這些可被此 Key 存取。</p>
        <ModelWhitelist providerId={providerId} value={models} onChange={setModels} />
      </div>

      <div className="form-section">
        <div className="mb-1 flex items-center gap-2">
          <span className="text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
            工具限制
          </span>
          {apps.length > 0 && (
            <span className="ml-auto rounded-full bg-fg/[0.08] px-2 py-0.5 text-[10px] font-medium text-fg/50">
              {apps.length}
            </span>
          )}
        </div>
        <p className="text-[11px] text-fg/30">全不選＝全部允許；選了的只允許這些工具使用。</p>
        <div className="tool-chips">
          {APP_META.map((m) => (
            <div key={m.id} className="tool-chip">
              <Logo name={m.id} size={15} className="shrink-0 opacity-80" />
              <span className="tool-chip-label">{m.label}</span>
              <Toggle
                size="sm"
                checked={apps.includes(m.id)}
                onChange={(v) =>
                  setApps((prev) =>
                    v ? [...prev, m.id] : prev.filter((x) => x !== m.id),
                  )
                }
              />
            </div>
          ))}
        </div>
      </div>

      {err && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs text-red-400">
          {err}
        </div>
      )}

      <div className="flex items-center justify-end gap-2.5 border-t border-fg/[0.06] pt-3">
        <button className="btn-ghost px-5 py-1.5 text-[13px]" onClick={props.onCancel}>
          取消
        </button>
        <button
          className="btn-primary px-6 py-1.5 text-[13px] disabled:opacity-40"
          disabled={save.isPending}
          onClick={() => save.mutate()}
        >
          {save.isPending ? "儲存中…" : props.initial ? "儲存" : "簽發"}
        </button>
      </div>
    </div>
  );
}
