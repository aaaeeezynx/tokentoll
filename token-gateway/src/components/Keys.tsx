import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type KeyInput, type LocalKey, type Provider } from "../lib/api";
import { Icon } from "./icons";
import { PopSelect } from "./PopSelect";
import { Toggle } from "./Toggle";
import { useConfirm } from "./Confirm";
import { APP_META, Logo } from "./logos";

function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

function fmtTokens(n: number): string {
  return n < 0 ? "不限" : n.toLocaleString();
}

/** 模型白名單：抓取目錄／映射勾選＋手動輸入。 */
function ModelWhitelist(props: {
  providerId: string;
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const qc = useQueryClient();
  const pid = props.providerId === "" ? null : Number(props.providerId);
  const catalog = useQuery({
    queryKey: ["catalog", pid],
    queryFn: () => api.catalogGet(pid as number),
    retry: false,
    enabled: pid != null,
  });
  const umodels = useQuery({
    queryKey: ["umodels", pid],
    queryFn: () => api.modelsList(pid as number),
    enabled: pid != null,
  });
  const [manual, setManual] = useState("");
  const [filter, setFilter] = useState("");
  const [msg, setMsg] = useState("");
  const fetch = useMutation({
    mutationFn: () => api.catalogFetch(pid as number),
    onSuccess: () => {
      setMsg("");
      void qc.invalidateQueries({ queryKey: ["catalog", pid] });
    },
    onError: (e) => setMsg(String(e)),
  });

  const avail = useMemo(() => {
    const map = new Map<string, string>();
    for (const m of catalog.data?.models || []) map.set(m.id, "目錄");
    for (const m of umodels.data || []) {
      if (!map.has(m.actual_model)) map.set(m.actual_model, "映射");
    }
    for (const v of props.value) {
      if (!map.has(v)) map.set(v, "已選");
    }
    return [...map.entries()].sort(([a], [b]) => (a < b ? -1 : 1));
  }, [catalog.data, umodels.data, props.value]);

  const toggle = (id: string) =>
    props.onChange(
      props.value.includes(id)
        ? props.value.filter((x) => x !== id)
        : [...props.value, id],
    );

  const filtered = filter.trim()
    ? avail.filter(
        ([id]) =>
          id.toLowerCase().includes(filter.trim().toLowerCase()) ||
          props.value.includes(id),
      )
    : avail;

  const addManual = () => {
    const v = manual.trim();
    if (v && !props.value.includes(v)) {
      props.onChange([...props.value, v]);
      setManual("");
    }
  };

  return (
    <div className="space-y-2">
      {props.value.length > 0 && (
        <>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-white/30">
            已選 {props.value.length} 個（空＝全部允許）
          </span>
          <span className="flex-1" />
          <button
            type="button"
            className="shrink-0 rounded-full px-2 py-0.5 text-[11px] text-white/35 transition-colors hover:bg-white/[0.07] hover:text-white/70"
            onClick={() => props.onChange([])}
          >
            清空
          </button>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {props.value.map((v) => (
            <button
              key={v}
              type="button"
              title="移除"
              onClick={() => toggle(v)}
              className="flex max-w-full items-center gap-1.5 rounded-full bg-white/[0.07] px-2.5 py-[3px] font-mono text-[11px] text-white/70 transition-colors hover:bg-red-500/15 hover:text-red-300"
            >
              <span className="truncate">{v}</span>
              <Icon name="x" size={10} className="shrink-0 text-white/40" />
            </button>
          ))}
        </div>
        </>
      )}

      {pid == null ? (
        <p className="text-[11px] text-white/30">
          先選上游來源，再從清單勾選；或手動輸入模型 ID。
        </p>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1 text-[11px] disabled:opacity-40"
              disabled={fetch.isPending}
              onClick={() => fetch.mutate()}
            >
              <Icon name="download" size={11} />
              {fetch.isPending ? "抓取中…" : "抓取目錄"}
            </button>
            <span className="truncate text-[11px] text-white/30">
              {catalog.data
                ? `${catalog.data.models.length} 個模型`
                : "尚未抓取"}
              {(umodels.data?.length ?? 0) > 0 &&
                ` · ${umodels.data?.length ?? 0} 個映射`}
            </span>
          </div>

          {msg && <p className="text-[11px] text-amber-300/80">{msg}</p>}

          {avail.length > 0 && (
            <>
              <div className="relative">
                <Icon
                  name="search"
                  size={12}
                  className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-white/25"
                />
                <input
                  className="field w-full py-1.5 pr-3 pl-7 text-[12px]"
                  placeholder="搜尋模型名稱…"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
              </div>
              <div className="max-h-40 space-y-0.5 overflow-y-auto rounded-lg border border-white/[0.06] bg-black/15 p-1">
                {filtered.length === 0 ? (
                  <p className="px-2 py-3 text-center text-[11px] text-white/25">無匹配</p>
                ) : (
                  filtered.map(([id, src]) => (
                    <label
                      key={id}
                      className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-[12px] text-white/60 transition-colors hover:bg-white/[0.05]"
                    >
                      <input
                        type="checkbox"
                        checked={props.value.includes(id)}
                        onChange={() => toggle(id)}
                        className="accent-[var(--mac-accent)]"
                      />
                      <span className="min-w-0 flex-1 truncate font-mono text-[11px]">{id}</span>
                      <span className="shrink-0 text-[10px] text-white/25">{src}</span>
                    </label>
                  ))
                )}
              </div>
            </>
          )}
        </>
      )}

      <div className="flex gap-1.5">
        <input
          className="field w-full px-2.5 py-1.5 text-[12px]"
          placeholder="手動輸入模型 ID"
          value={manual}
          onChange={(e) => setManual(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") addManual();
          }}
        />
        <button
          type="button"
          className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1.5 text-[12px] disabled:opacity-40"
          disabled={manual.trim() === ""}
          onClick={addManual}
        >
          <Icon name="plus" size={11} />
          新增
        </button>
      </div>
    </div>
  );
}

function KeyForm(props: {
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
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
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
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
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
          <span className="text-[11px] font-semibold tracking-wider text-white/40 uppercase">
            模型白名單
          </span>
          {models.length > 0 && (
            <span className="ml-auto rounded-full bg-white/[0.08] px-2 py-0.5 text-[10px] font-medium text-white/50">
              {models.length}
            </span>
          )}
        </div>
        <p className="text-[11px] text-white/30">空＝全部允許；選了的只有這些可被此 Key 存取。</p>
        <ModelWhitelist providerId={providerId} value={models} onChange={setModels} />
      </div>

      <div className="form-section">
        <div className="mb-1 flex items-center gap-2">
          <span className="text-[11px] font-semibold tracking-wider text-white/40 uppercase">
            工具限制
          </span>
          {apps.length > 0 && (
            <span className="ml-auto rounded-full bg-white/[0.08] px-2 py-0.5 text-[10px] font-medium text-white/50">
              {apps.length}
            </span>
          )}
        </div>
        <p className="text-[11px] text-white/30">全不選＝全部允許；選了的只允許這些工具使用。</p>
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

      <div className="flex items-center justify-end gap-2.5 border-t border-white/[0.06] pt-3">
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

/** macOS Alert：簽發／輪換成功後顯示完整 Key。 */
function SecretAlert(props: {
  secret: string;
  reason: "new" | "rotated";
  onClose: () => void;
}) {
  const [closing, setClosing] = useState(false);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 200);
  };
  return (
    <div
      className={`fixed inset-0 z-[90] flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div className={`w-full max-w-md glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}>
        <div className="flex items-start gap-3">
          <span
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px]"
            style={{
              background: "color-mix(in srgb, var(--mac-accent) 18%, transparent)",
              color: "var(--mac-accent)",
            }}
          >
            <Icon name={props.reason === "new" ? "key-fill" : "refresh"} size={20} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[16px] font-semibold tracking-tight text-white">
              {props.reason === "new" ? "Key 已簽發" : "Key 已輪換"}
            </div>
            <p className="mt-1 text-[13px] leading-relaxed text-white/55">
              此 Key 已存於本機，可隨時在列表點「查看」再次顯示與複製。
            </p>
          </div>
          <button
            className="shrink-0 rounded-full bg-white/[0.055] p-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
        </div>
        <div className="mt-4 rounded-xl border border-white/[0.08] bg-black/25 p-3">
          <div className="mb-1.5 text-[11px] font-medium text-white/35">完整 Key</div>
          <code className="block max-h-28 select-all overflow-y-auto break-all font-mono text-[13px] leading-relaxed text-white/90">
            {props.secret}
          </code>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button className="btn-ghost px-4 py-2 text-sm" onClick={requestClose}>
            關閉
          </button>
          <button
            className="btn-primary flex items-center gap-1.5 px-4 py-2 text-sm"
            onClick={() => copyText(props.secret)}
          >
            <Icon name="copy" size={14} />
            複製 Key
          </button>
        </div>
      </div>
    </div>
  );
}

/** 檢視 Key：顯示完整 Key（可複製）＋詳細資訊＋編輯入口。 */
function ViewDialog(props: { item: LocalKey; onClose: () => void; onEdit: () => void }) {
  const k = props.item;
  const [revealed, setRevealed] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reveal = async () => {
    setLoading(true);
    setErr(null);
    try {
      setRevealed(await api.keyReveal(k.id));
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[85] flex items-center justify-center glass-backdrop p-4"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div className="w-full max-w-md glass-panel pop-in p-5">
        <div className="flex items-center gap-2.5">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
            <Icon name="key" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[15px] font-semibold text-white">{k.name}</div>
            <div className="truncate font-mono text-[11px] text-white/30">
              {k.key_prefix} · {k.enabled ? "已啟用" : "已停用"}
            </div>
          </div>
          <button
            className="rounded-full bg-white/[0.055] p-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            onClick={props.onClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
        </div>

        <div className="mt-4 space-y-3 text-[13px]">
          <div className="rounded-xl border border-white/[0.07] bg-black/20 p-3">
            <div className="mb-1.5 text-[11px] font-medium text-white/40">完整 Key</div>
            {revealed ? (
              <>
                <code className="block max-h-28 select-all overflow-y-auto break-all rounded-lg bg-black/30 p-2.5 font-mono text-white/90">
                  {revealed}
                </code>
                <div className="mt-2 flex gap-2">
                  <button
                    className="btn-primary flex items-center gap-1.5 px-3 py-1.5 text-xs"
                    onClick={() => copyText(revealed)}
                  >
                    <Icon name="copy" size={12} />
                    複製完整 Key
                  </button>
                  <button
                    className="btn-ghost px-3 py-1.5 text-xs"
                    onClick={() => setRevealed(null)}
                  >
                    隱藏
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="mb-2 text-white/40">簽發後仍可隨時查看明文（本機存儲）。</p>
                {err && <p className="mb-2 text-red-400">{err}</p>}
                <button
                  className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
                  disabled={loading}
                  onClick={reveal}
                >
                  <Icon name="eye" size={12} />
                  {loading ? "載入中…" : "顯示完整 Key"}
                </button>
              </>
            )}
          </div>

          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">來源</span>
            <span className="text-white/75">{k.provider_name || "未綁定"}</span>
          </div>
          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">用量</span>
            <span className="text-white/75">
              {k.used_tokens.toLocaleString()} / {fmtTokens(k.quota_tokens)}
            </span>
          </div>
          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">限流</span>
            <span className="text-white/75">
              {k.rate_limit_qpm <= 0 ? "不限" : `${k.rate_limit_qpm}/分鐘`}
            </span>
          </div>

          {k.allowed_models.length > 0 && (
            <div>
              <div className="mb-1.5 text-[11px] font-medium text-white/40">模型白名單</div>
              <div className="flex flex-wrap gap-1">
                {k.allowed_models.map((m) => (
                  <span
                    key={m}
                    className="rounded-full bg-white/[0.06] px-2 py-0.5 font-mono text-[11px] text-white/60"
                  >
                    {m}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button className="btn-ghost px-4 py-1.5 text-sm" onClick={props.onClose}>
            關閉
          </button>
          <button className="btn-primary px-4 py-1.5 text-sm" onClick={props.onEdit}>
            編輯
          </button>
        </div>
      </div>
    </div>
  );
}

function KeySheet(props: {
  initial: LocalKey | null;
  providers: Provider[];
  onClose: () => void;
  onSaved: () => void;
  onCreatedSecret: (s: string) => void;
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
            className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/65">
            <Icon name="key" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight text-white">
            {props.initial ? `編輯「${props.initial.name}」` : "簽發 Key"}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto overflow-x-clip px-5 py-5">
          <KeyForm
            key={props.initial?.id ?? "new"}
            initial={props.initial}
            providers={props.providers}
            onDone={() => {
              props.onSaved();
              requestClose();
            }}
            onCancel={requestClose}
            onCreatedSecret={props.onCreatedSecret}
          />
        </div>
      </div>
    </div>
  );
}

export default function KeysPage() {
  const qc = useQueryClient();
  const keys = useQuery({ queryKey: ["keys"], queryFn: api.keysList });
  const providers = useQuery({
    queryKey: ["providers"],
    queryFn: api.providersList,
  });
  const [editing, setEditing] = useState<LocalKey | "new" | null>(null);
  const [secretAlert, setSecretAlert] = useState<{
    secret: string;
    reason: "new" | "rotated";
  } | null>(null);
  const [viewKey, setViewKey] = useState<LocalKey | null>(null);
  const [actionErr, setActionErr] = useState("");

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["keys"] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
  };

  const toggle = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      api.keySetEnabled(id, enabled),
    onSuccess: refresh,
  });
  const del = useMutation({
    mutationFn: (id: number) => api.keyDelete(id),
    onSuccess: refresh,
  });
  const rotate = useMutation({
    mutationFn: (id: number) => api.keyRotate(id),
    onSuccess: (created) => {
      setActionErr("");
      setSecretAlert({ secret: created.secret, reason: "rotated" });
      refresh();
    },
    onError: (e) => setActionErr(String(e)),
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  return (
    <div className="space-y-4">
      {secretAlert && (
        <SecretAlert
          secret={secretAlert.secret}
          reason={secretAlert.reason}
          onClose={() => setSecretAlert(null)}
        />
      )}
      {editing && (
        <KeySheet
          initial={editing === "new" ? null : editing}
          providers={providers.data || []}
          onClose={() => setEditing(null)}
          onSaved={refresh}
          onCreatedSecret={(s) => setSecretAlert({ secret: s, reason: "new" })}
        />
      )}
      {viewKey && (
        <ViewDialog
          item={viewKey}
          onClose={() => setViewKey(null)}
          onEdit={() => {
            setViewKey(null);
            setEditing(viewKey);
          }}
        />
      )}

      <div className="glass p-5">
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <span className="flex h-9 w-9 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
              <Icon name="key" size={17} />
            </span>
            <div>
              <div className="text-[15px] font-semibold tracking-tight text-white">本地 Key</div>
              <div className="text-[11px] text-white/30">
                {keys.data?.length ?? 0} 個 · 綁定上游後簽發
              </div>
            </div>
          </div>
          <button
            className="btn-primary flex items-center gap-1.5 px-4 py-1.5 text-sm"
            onClick={() => setEditing("new")}
          >
            <Icon name="plus" size={14} />
            簽發 Key
          </button>
        </div>

        {actionErr && <p className="mb-2 text-xs text-red-400">{actionErr}</p>}

        {keys.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : (keys.data || []).length === 0 ? (
          <p className="text-sm text-white/30">
            還沒有 Key。先在「上游來源」添加來源，再回來簽發。
          </p>
        ) : (
          <div className="mac-rows">
            {(keys.data || []).map((k, i) => {
              const pct =
                k.quota_tokens > 0
                  ? Math.min(100, (k.used_tokens / k.quota_tokens) * 100)
                  : 0;
              const remaining =
                k.quota_tokens > 0 ? k.quota_tokens - k.used_tokens : null;
              const daysSinceCreation = Math.max(
                1,
                (Date.now() - k.created_at) / 86400000,
              );
              const avgDailyTokens = k.used_tokens / daysSinceCreation;
              const daysLeft =
                remaining !== null && avgDailyTokens > 0
                  ? Math.round(remaining / avgDailyTokens)
                  : null;
              return (
                <div
                  key={k.id}
                  className="key-row anim-rise"
                  style={{ animationDelay: `${Math.min(i * 40, 300)}ms` }}
                >
                  <div className="key-main">
                    <div className="key-title">
                      <span className="truncate">{k.name}</span>
                      {!k.enabled && (
                        <span className="rounded-full bg-white/[0.07] px-2 py-0.5 text-[10px] text-white/35">
                          已停用
                        </span>
                      )}
                    </div>
                    <div className="key-meta">
                      <code>{k.key_prefix}</code>
                      <span className="mx-1.5 text-white/20">·</span>
                      {k.provider_name || "未綁定"}
                      {k.quota_tokens > 0 && (
                        <>
                          <span className="mx-1.5 text-white/20">·</span>
                          {k.used_tokens.toLocaleString()} / {fmtTokens(k.quota_tokens)}
                          {daysLeft !== null && (
                            <span className="ml-1.5">
                              {daysLeft <= 3 ? (
                                <span className="text-red-400">
                                  {daysLeft <= 0 ? "已耗盡" : `剩 ${daysLeft} 天`}
                                </span>
                              ) : (
                                `約 ${daysLeft} 天`
                              )}
                            </span>
                          )}
                        </>
                      )}
                      {k.quota_tokens <= 0 && (
                        <>
                          <span className="mx-1.5 text-white/20">·</span>
                          用量 {k.used_tokens.toLocaleString()}
                        </>
                      )}
                      <span className="mx-1.5 text-white/20">·</span>
                      {k.rate_limit_qpm <= 0 ? "不限流" : `${k.rate_limit_qpm}/分`}
                    </div>
                    {k.quota_tokens > 0 && (
                      <div className="mt-2 h-1 w-40 max-w-full overflow-hidden rounded-full bg-white/[0.06]">
                        <div
                          className={`h-full rounded-full bar-anim ${
                            pct > 95
                              ? "bg-red-400/80"
                              : pct > 80
                                ? "bg-amber-400/80"
                                : "bg-[#30d158]/80"
                          }`}
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                    )}
                  </div>
                  <div className="key-actions shrink-0">
                    <button
                      className="rounded-full p-2 text-white/40 transition-colors hover:bg-white/10 hover:text-white"
                      title="查看／複製"
                      aria-label="查看／複製"
                      onClick={() => setViewKey(k)}
                    >
                      <Icon name="eye" size={14} />
                    </button>
                    <button
                      className="rounded-full p-2 text-white/40 transition-colors hover:bg-white/10 hover:text-white"
                      title="編輯"
                      aria-label="編輯"
                      onClick={() => setEditing(k)}
                    >
                      <Icon name="pencil" size={14} />
                    </button>
                    <button
                      className="rounded-full p-2 text-white/40 transition-colors hover:bg-white/10 hover:text-white disabled:opacity-30"
                      disabled={rotate.isPending}
                      title="輪換"
                      aria-label="輪換"
                      onClick={() =>
                        ask(`輪換 Key ${k.name}？`, () => rotate.mutate(k.id), {
                          message: "舊 Key 立即失效，新 Key 會以警示視窗顯示一次。",
                          confirmLabel: "輪換",
                        })
                      }
                    >
                      <Icon name="refresh" size={14} />
                    </button>
                    <button
                      className="rounded-full p-2 text-white/40 transition-colors hover:bg-white/10 hover:text-white"
                      title={k.enabled ? "停用" : "啟用"}
                      aria-label={k.enabled ? "停用" : "啟用"}
                      onClick={() => toggle.mutate({ id: k.id, enabled: !k.enabled })}
                    >
                      <Icon name="power" size={14} />
                    </button>
                    <button
                      className="rounded-full p-2 text-white/40 transition-colors hover:bg-red-500/10 hover:text-red-400"
                      title="刪除"
                      aria-label="刪除"
                      onClick={() =>
                        ask(`刪除 Key ${k.name}？`, () => del.mutate(k.id), {
                          message: "已分發的 Key 將立即失效。",
                        })
                      }
                    >
                      <Icon name="trash" size={14} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <p className="text-xs text-white/25">
        終端機使用範例：$env:TOKEN_GATEWAY_KEY="sk-local-…" 後把工具 base_url
        指向本網關（上游來源頁一鍵切換）。
      </p>
      {confirmDialog}
    </div>
  );
}