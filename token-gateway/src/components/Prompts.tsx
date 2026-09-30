/**
 * 提示詞預設集（P3.2，對標 cc-switch 的 Prompts Management）。
 *
 * 逐工具分開管理（Claude Code／Codex／OpenCode），每個工具**同時只能有一個啟用**；
 * 啟用後內容寫進該工具的檔案（`~/.claude/CLAUDE.md`、`~/.codex/AGENTS.md`、
 * `~/.config/opencode/AGENTS.md`）。
 *
 * **智慧回填**：切換預設集之前，如果現行檔案與資料庫裡那個預設集不同
 * （代表你手改過），會先把檔案內容存回舊的預設集再切 —— 手改不會被吃掉。
 * 檔案被手改過時，面板上會出現提示。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../lib/api";
import { Icon } from "../components/icons";
import { useConfirm } from "../components/Confirm";
import { PromptEditor } from "./prompts/PromptEditor";

export default function PromptsPage() {
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [app, setApp] = useState("codex");
  const [editing, setEditing] = useState<"new" | number | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [showFile, setShowFile] = useState(false);

  const apps = useQuery({ queryKey: ["prompt_apps"], queryFn: api.promptApps });
  const state = useQuery({
    queryKey: ["prompt_state", app],
    queryFn: () => api.promptState(app),
  });
  const list = useQuery({
    queryKey: ["prompt_list", app],
    queryFn: () => api.promptList(app),
  });
  const live = useQuery({
    queryKey: ["prompt_live", app],
    queryFn: () => api.promptLive(app),
    enabled: showFile,
  });

  // 首次匯入會在讀狀態時發生，讀完要刷新清單
  useEffect(() => {
    if (state.data) void qc.invalidateQueries({ queryKey: ["prompt_list", app] });
  }, [state.data, app, qc]);

  const done = (r: { backfilled_into: string; wrote_file: boolean; imported: string; backup: string | null; message: string }, prefix: string) => {
    const bits = [prefix];
    if (r.imported) bits.push(`首次匯入「${r.imported}」`);
    if (r.backfilled_into) bits.push(`已把檔案內容回填到「${r.backfilled_into}」`);
    if (r.wrote_file) bits.push("已寫入檔案");
    if (r.backup) bits.push(`已備份 ${r.backup}`);
    if (!r.wrote_file && !r.backfilled_into && !r.imported) bits.push("檔案無變動");
    if (r.message) bits.push(r.message);
    setMsg(bits.join("；"));
    setErr("");
    void qc.invalidateQueries({ queryKey: ["prompt_list", app] });
    void qc.invalidateQueries({ queryKey: ["prompt_state", app] });
    void qc.invalidateQueries({ queryKey: ["prompt_live", app] });
  };

  const activate = useMutation({
    mutationFn: (id: number) => api.promptActivate(id),
    onSuccess: (r) => done(r, "已啟用"),
    onError: (e) => setErr(`啟用失敗：${String(e)}`),
  });
  const deactivate = useMutation({
    mutationFn: () => api.promptDeactivate(app),
    onSuccess: () => {
      setMsg("已停用（檔案內容留著，只是不再由這裡管理）");
      void qc.invalidateQueries({ queryKey: ["prompt_list", app] });
      void qc.invalidateQueries({ queryKey: ["prompt_state", app] });
    },
    onError: (e) => setErr(`停用失敗：${String(e)}`),
  });
  const del = useMutation({
    mutationFn: (id: number) => api.promptDelete(id),
    onSuccess: () => {
      setMsg("已刪除");
      void qc.invalidateQueries({ queryKey: ["prompt_list", app] });
    },
    onError: (e) => setErr(`刪除失敗：${String(e)}`),
  });
  const sync = useMutation({
    mutationFn: () => api.promptSync(app),
    onSuccess: (r) => done(r, "已重新同步"),
    onError: (e) => setErr(`同步失敗：${String(e)}`),
  });

  const presets = list.data ?? [];
  const st = state.data;
  const editingPreset =
    typeof editing === "number" ? presets.find((p) => p.id === editing) ?? null : null;

  return (
    <div className="space-y-4">
      {dialog}
      <div className="glass p-5">
        <div className="mb-3 flex flex-wrap items-center gap-2.5">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
            <Icon name="pencil" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[15px] font-semibold tracking-tight text-white">
              提示詞預設集
            </div>
            <div className="min-w-0 truncate font-mono text-[11px] text-white/30">
              {st ? `${st.path}${st.file_exists ? `（${st.live_bytes} bytes）` : "（檔案不存在）"}` : ""}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
              disabled={sync.isPending}
              onClick={() => sync.mutate()}
              title="不改變設定，只跑一次「回填 → 寫檔」"
            >
              <Icon name="refresh" size={12} />
              重新同步
            </button>
            <button
              className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs"
              onClick={() => setShowFile((v) => !v)}
            >
              <Icon name="eye" size={12} />
              {showFile ? "收起檔案" : "看檔案"}
            </button>
            <button
              className="btn-primary flex items-center gap-1.5 px-4 py-1.5 text-sm"
              onClick={() => setEditing("new")}
            >
              <Icon name="plus" size={13} />
              新增
            </button>
          </div>
        </div>

        {/* 工具頁籤（cc-switch：提示詞逐工具分開管理） */}
        <div className="mb-3 flex flex-wrap gap-1.5">
          {(apps.data ?? []).map((a) => (
            <button
              key={a.app}
              className={`rounded-lg px-3 py-1.5 text-[12px] transition-colors ${
                a.app === app
                  ? "bg-white/[0.14] text-white"
                  : "bg-white/[0.05] text-white/55 hover:bg-white/[0.09]"
              }`}
              onClick={() => {
                setApp(a.app);
                setEditing(null);
                setMsg("");
                setErr("");
              }}
              title={a.installed ? a.path : `${a.path}（這個工具好像沒安裝）`}
            >
              {a.display}
              {!a.installed && <span className="ml-1 text-[10px] text-amber-300/70">未安裝</span>}
            </button>
          ))}
        </div>

        {st?.dirty && (
          <p className="mb-2 text-[11px] text-amber-300/80">
            檔案內容與「{st.active_name || "目前預設集"}」不同 —— 看起來你手改過。
            切換到別的預設集時，會先把檔案內容存回這一個（不會遺失）。
          </p>
        )}
        {msg && <p className="pb-2 text-xs text-emerald-400/80">{msg}</p>}
        {err && <p className="pb-2 text-xs break-words text-red-400">{err}</p>}

        {showFile && (
          <pre className="mb-3 max-h-56 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] whitespace-pre-wrap text-white/60">
            {live.isPending ? "讀取中…" : live.data || "（檔案不存在或內容為空）"}
          </pre>
        )}

        {list.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : presets.length === 0 ? (
          <p className="text-[13px] leading-relaxed text-white/35">
            還沒有任何預設集。按「新增」建立一份；如果這個工具的檔案本來就有內容，
            第一次打開這一頁時會自動收成一個「現有內容」的預設集。
          </p>
        ) : (
          <div className="space-y-2">
            {presets.map((p) => (
              <div
                key={p.id}
                className="rounded-xl border border-white/[0.06] p-3.5 transition-colors hover:border-white/10"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-semibold text-white/85">
                    {p.name}
                    {p.active && (
                      <span
                        className="ml-2 rounded px-1.5 py-px text-[10px]"
                        style={{ background: "rgba(48,209,88,0.14)", color: "#30d158" }}
                      >
                        啟用中
                      </span>
                    )}
                  </span>
                  <span className="shrink-0 text-[10px] text-white/25">
                    {p.content.length} 字元
                  </span>
                  <button
                    className={`shrink-0 rounded-lg px-2.5 py-1 text-[11px] transition-colors ${
                      p.active
                        ? "bg-white/[0.14] text-white"
                        : "bg-white/[0.05] text-white/60 hover:bg-white/[0.1]"
                    }`}
                    disabled={p.active || activate.isPending}
                    onClick={() => activate.mutate(p.id)}
                    title={p.active ? "已經啟用中" : "啟用（會寫進工具檔案）"}
                  >
                    {p.active ? "使用中" : "啟用"}
                  </button>
                  <button
                    className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-white/10 hover:text-white"
                    title="編輯"
                    onClick={() => setEditing(p.id)}
                  >
                    <Icon name="pencil" size={13} />
                  </button>
                  <button
                    className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
                    title={p.active ? "啟用中不能刪除" : "刪除"}
                    onClick={() =>
                      ask(`刪除預設集「${p.name}」？`, () => del.mutate(p.id), {
                        message: "只會刪掉這裡的預設集，工具檔案內容不會被動到。",
                      })
                    }
                  >
                    <Icon name="trash" size={13} />
                  </button>
                </div>
                <p className="mt-1 truncate font-mono text-[11px] text-white/30">
                  {p.content.split("\n")[0] || "（空內容）"}
                </p>
              </div>
            ))}
            {presets.some((p) => p.active) && (
              <button
                className="btn-ghost px-3 py-1.5 text-[11px]"
                disabled={deactivate.isPending}
                onClick={() => deactivate.mutate()}
              >
                停用目前預設集
              </button>
            )}
          </div>
        )}

        <p className="pt-3 text-[11px] leading-relaxed text-white/25">
          Claude Code 寫 <span className="font-mono">~/.claude/CLAUDE.md</span>；
          Codex 寫 <span className="font-mono">~/.codex/AGENTS.md</span>；
          OpenCode 寫 <span className="font-mono">~/.config/opencode/AGENTS.md</span>。
          每次改寫前都會備份到 <span className="font-mono">backups/&lt;工具&gt;-prompt/</span>（保留 5 份）。
        </p>
      </div>

      {editing !== null && (
        <PromptEditor
          app={app}
          initial={editingPreset}
          onClose={() => setEditing(null)}
          onSaved={(saved) => {
            setEditing(null);
            setMsg(saved.active ? `已儲存並同步「${saved.name}」` : `已儲存「${saved.name}」（尚未啟用）`);
            setErr("");
            void qc.invalidateQueries({ queryKey: ["prompt_list", app] });
            void qc.invalidateQueries({ queryKey: ["prompt_state", app] });
            void qc.invalidateQueries({ queryKey: ["prompt_live", app] });
          }}
        />
      )}
    </div>
  );
}
