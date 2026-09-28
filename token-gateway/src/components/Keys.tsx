import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type LocalKey } from "../lib/api";
import { Icon } from "./icons";
import { useConfirm } from "./Confirm";
import { fmtTokens } from "./keys/shared";
import { KeySheet } from "./keys/KeySheet";
import { SecretAlert } from "./keys/SecretAlert";
import { ViewDialog } from "./keys/ViewDialog";

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
