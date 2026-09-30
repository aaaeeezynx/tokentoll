/**
 * 會話管理（P3.4，對標 cc-switch 的 Session Manager）。
 *
 * 左邊清單（搜尋／依工具篩選／重新整理／批次刪除），右邊詳情（標題、最後活動、
 * 專案目錄、**續聊指令**、對話全文）。
 *
 * 續聊在 Windows 上是**複製指令到剪貼簿**（cc-switch：非 macOS 只複製，
 * 由使用者自己貼進終端機）—— 我們不擅自開終端機。
 *
 * 刪除只適用 Claude／Codex 的檔案；OpenCode 的會話在它自己的 SQLite 裡，
 * 這裡只讀不刪（與 cc-switch「沒有本機檔案路徑的不能刪」同一條規矩）。
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, type SessionSummary } from "../lib/api";
import { Icon } from "../components/icons";
import { useConfirm } from "../components/Confirm";
import { PopSelect } from "../components/PopSelect";

function relTime(ms: number): string {
  if (!ms) return "—";
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return `${s} 秒前`;
  if (s < 3600) return `${Math.floor(s / 60)} 分鐘前`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小時前`;
  return `${Math.floor(s / 86400)} 天前`;
}

function basename(p: string): string {
  const t = p.replace(/[\\/]+$/, "");
  return t.split(/[\\/]/).pop() || t;
}

const ROLE_STYLE: Record<string, string> = {
  user: "rgba(48,209,88,0.10)",
  assistant: "rgba(10,132,255,0.10)",
  system: "rgba(255,159,10,0.10)",
  tool: "rgba(191,90,242,0.10)",
};
const ROLE_LABEL: Record<string, string> = {
  user: "使用者",
  assistant: "AI",
  system: "系統",
  tool: "工具",
};

export default function SessionsPage() {
  const { dialog, ask } = useConfirm();
  const [appFilter, setAppFilter] = useState("");
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<SessionSummary | null>(null);
  const [batch, setBatch] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  const list = useQuery({ queryKey: ["sessions_scan"], queryFn: () => api.sessionsScan() });
  const apps = useQuery({ queryKey: ["sessions_apps"], queryFn: api.sessionsApps });
  const transcript = useQuery({
    queryKey: ["sessions_read", sel?.app, sel?.id],
    queryFn: () => api.sessionsRead(sel!.app, sel!.id, sel!.path),
    enabled: sel != null,
  });

  const rows = useMemo(() => {
    const all = list.data ?? [];
    const s = q.trim().toLowerCase();
    return all.filter((x) => {
      if (appFilter && x.app !== appFilter) return false;
      if (!s) return true;
      return (
        x.id.toLowerCase().includes(s) ||
        x.title.toLowerCase().includes(s) ||
        x.project_dir.toLowerCase().includes(s) ||
        x.path.toLowerCase().includes(s)
      );
    });
  }, [list.data, q, appFilter]);

  const del = useMutation({
    mutationFn: (targets: [string, string][]) => api.sessionsDelete(targets),
    onSuccess: (r) => {
      setMsg(
        `已刪除 ${r.deleted.length} 則` +
          (r.failed.length > 0 ? `，失敗 ${r.failed.length} 則（${r.failed[0][1]}）` : "") +
          `。${r.note}`,
      );
      setErr("");
      setPicked(new Set());
      setSel(null);
      void list.refetch();
    },
    onError: (e) => setErr(String(e)),
  });

  const appLabel = (a: string) =>
    apps.data?.find((x) => x.app === a)?.display ?? a;
  const key = (s: SessionSummary) => `${s.app}|${s.path}`;
  const togglePick = (s: SessionSummary) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (next.has(key(s))) next.delete(key(s));
      else next.add(key(s));
      return next;
    });

  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text);
    setMsg(`已複製：${text}`);
  };

  return (
    <div className="space-y-4">
      {dialog}
      <div className="glass p-5">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <input
            className="field min-w-[200px] flex-1 px-2.5 py-1.5 text-[12px]"
            placeholder="搜尋標題、專案目錄、session id 或檔案路徑…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQ("");
            }}
          />
          <div className="w-40">
            <PopSelect
              value={appFilter}
              onChange={setAppFilter}
              options={[
                { value: "", label: "全部工具" },
                ...(apps.data ?? []).map((a) => ({ value: a.app, label: a.display })),
              ]}
            />
          </div>
          <button
            className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
            disabled={list.isFetching}
            onClick={() => void list.refetch()}
          >
            <Icon name="refresh" size={12} />
            {list.isFetching ? "掃描中…" : "重新整理"}
          </button>
          <button
            className={`btn-ghost px-3 py-1.5 text-xs ${batch ? "text-white" : ""}`}
            onClick={() => {
              setBatch((v) => !v);
              setPicked(new Set());
            }}
          >
            {batch ? "結束批次" : "批次模式"}
          </button>
          {batch && (
            <>
              <button
                className="btn-ghost px-2.5 py-1.5 text-[11px]"
                onClick={() => setPicked(new Set(rows.filter((r) => r.deletable).map(key)))}
              >
                全選（{rows.filter((r) => r.deletable).length}）
              </button>
              <button
                className="btn-ghost px-2.5 py-1.5 text-[11px]"
                onClick={() => setPicked(new Set())}
              >
                清除
              </button>
              <button
                className="btn-ghost flex items-center gap-1 px-3 py-1.5 text-[11px] text-red-400 disabled:opacity-40"
                disabled={picked.size === 0 || del.isPending}
                onClick={() =>
                  ask(`刪除 ${picked.size} 則會話？`, () => {
                    const targets = (list.data ?? [])
                      .filter((s) => picked.has(key(s)))
                      .map((s) => [s.app, s.path] as [string, string]);
                    del.mutate(targets);
                  }, {
                    message:
                      "會直接刪除這些會話檔（無法復原）。已回填的用量統計不會被撤銷。",
                  })
                }
              >
                <Icon name="trash" size={11} />
                批次刪除
              </button>
            </>
          )}
        </div>

        {msg && <p className="pb-2 text-[11px] break-all text-emerald-400/80">{msg}</p>}
        {err && <p className="pb-2 text-[11px] break-all text-red-400">{err}</p>}

        <div className="grid items-start gap-4 lg:grid-cols-2">
          {/* 左：清單 */}
          <div className="min-w-0">
            <p className="pb-2 text-[11px] text-white/30">
              {list.isPending ? "掃描中…" : `${rows.length} 則（新到舊）`}
            </p>
            <div className="max-h-[62vh] space-y-1.5 overflow-y-auto pr-0.5">
              {rows.map((s) => (
                <div
                  key={key(s)}
                  onClick={() => {
                    if (batch) togglePick(s);
                    else setSel(s);
                  }}
                  className={`cursor-pointer rounded-lg border px-3 py-2 transition-colors ${
                    sel && key(sel) === key(s)
                      ? "border-white/20 bg-white/[0.06]"
                      : "border-white/[0.06] hover:border-white/12"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {batch && (
                      <input
                        type="checkbox"
                        className="accent-[#0A84FF]"
                        checked={picked.has(key(s))}
                        onChange={() => togglePick(s)}
                        onClick={(e) => e.stopPropagation()}
                      />
                    )}
                    <span className="min-w-0 flex-1 truncate text-[13px] text-white/80">
                      {s.title}
                    </span>
                    <span className="shrink-0 rounded bg-white/[0.06] px-1.5 py-px text-[10px] text-white/40">
                      {appLabel(s.app)}
                    </span>
                  </div>
                  <div className="flex items-center gap-2 pt-0.5 text-[10px] text-white/30">
                    <span className="min-w-0 flex-1 truncate">
                      {s.project_dir ? basename(s.project_dir) : "—"}
                    </span>
                    <span className="shrink-0">{relTime(s.last_active_ms)}</span>
                  </div>
                </div>
              ))}
              {!list.isPending && rows.length === 0 && (
                <p className="text-[13px] text-white/35">沒有符合的會話。</p>
              )}
            </div>
          </div>

          {/* 右：詳情 */}
          <div className="min-w-0">
            {!sel ? (
              <p className="text-[13px] text-white/35">從左邊選一則會話看內容。</p>
            ) : (
              <div className="rounded-xl border border-white/[0.06] p-3.5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-semibold text-white/85">
                    {sel.title}
                  </span>
                  <span className="rounded bg-white/[0.06] px-1.5 py-px text-[10px] text-white/40">
                    {appLabel(sel.app)}
                  </span>
                  {sel.deletable ? (
                    <button
                      className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
                      title="刪除這則會話（直接刪檔，無法復原）"
                      onClick={() =>
                        ask(`刪除「${sel.title}」？`, () => del.mutate([[sel.app, sel.path]]), {
                          message:
                            "會直接刪除這個會話檔（無法復原）。已回填的用量統計不會被撤銷。",
                        })
                      }
                    >
                      <Icon name="trash" size={13} />
                    </button>
                  ) : (
                    <span className="shrink-0 text-[10px] text-white/25" title="OpenCode 的會話在它自己的資料庫裡">
                      不可刪除
                    </span>
                  )}
                </div>

                <div className="space-y-0.5 pt-1.5 text-[11px] text-white/35">
                  <div>最後活動：{new Date(sel.last_active_ms).toLocaleString()}</div>
                  {sel.project_dir && (
                    <div className="flex items-center gap-1.5">
                      <span className="truncate" title={sel.project_dir}>
                        專案：{sel.project_dir}
                      </span>
                      <button
                        className="shrink-0 text-white/30 hover:text-white/70"
                        title="複製完整路徑"
                        onClick={() => copy(sel.project_dir)}
                      >
                        <Icon name="copy" size={11} />
                      </button>
                    </div>
                  )}
                  <div className="font-mono text-[10px] break-all text-white/25">
                    {sel.path}
                  </div>
                </div>

                {sel.resume_command && (
                  <div className="mt-2 flex items-center gap-2 rounded-lg bg-black/25 px-2.5 py-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-white/70">
                      {sel.resume_command}
                    </span>
                    <button
                      className="btn-ghost shrink-0 px-2.5 py-1 text-[11px]"
                      onClick={() => copy(sel.resume_command)}
                      title="複製續聊指令（Windows 上請自己貼進終端機）"
                    >
                      複製指令
                    </button>
                  </div>
                )}

                <div className="mt-3 max-h-[46vh] space-y-1.5 overflow-y-auto pr-0.5">
                  {transcript.isPending ? (
                    <p className="text-[12px] text-white/30">讀取對話中…</p>
                  ) : transcript.isError ? (
                    <p className="text-[12px] break-words text-white/50">
                      讀取失敗：{String(transcript.error)}
                    </p>
                  ) : (transcript.data?.length ?? 0) === 0 ? (
                    <p className="text-[12px] text-white/30">這則會話沒有可顯示的訊息。</p>
                  ) : (
                    transcript.data?.map((m, i) => (
                      <div
                        key={i}
                        className="rounded-lg px-2.5 py-2"
                        style={{ background: ROLE_STYLE[m.role] ?? "rgba(255,255,255,0.04)" }}
                      >
                        <div className="pb-0.5 text-[10px] text-white/40">
                          {ROLE_LABEL[m.role] ?? m.role}
                          {m.ts > 0 && (
                            <span className="pl-2 text-white/25">
                              {new Date(m.ts).toLocaleTimeString()}
                            </span>
                          )}
                        </div>
                        <div className="text-[12px] leading-relaxed break-words whitespace-pre-wrap text-white/75">
                          {m.text.length > 4000 ? `${m.text.slice(0, 4000)}…` : m.text}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>
        </div>

        <p className="pt-3 text-[11px] leading-relaxed text-white/25">
          Claude Code 讀 <span className="font-mono">~/.claude/projects/</span>；
          Codex 讀 <span className="font-mono">~/.codex/sessions/</span> 與
          <span className="font-mono"> archived_sessions/</span>；
          OpenCode 讀它自己的資料庫（<span className="font-mono">只讀不刪</span>）。
          長對話只顯示最後 400 則。
        </p>
      </div>
    </div>
  );
}
