/**
 * MCP 管理（P3.1，對標 cc-switch 的 MCP Management）。
 *
 * 一個統一的面板，每個伺服器可以**逐工具**開啟／關閉；開啟＝寫進那個工具的設定檔，
 * 關閉＝從那個檔案移除。**只動資料庫管得到的伺服器**，使用者手寫的不碰
 * （與 cc-switch 同規則，後端 `mcp/apply.rs` 有測試釘住）。
 *
 * 我們同步的三個工具就是可以接管的三個（Claude Code／Codex／OpenCode）——
 * 能寫設定檔的工具才可能同步 MCP，所以兩份清單共用 `TAKEOVER_APPS`。
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type McpServer, type McpSyncOutcome } from "../lib/api";
import { Icon } from "./icons";
import { useConfirm } from "./Confirm";
import { McpForm } from "./mcp/McpForm";
import { McpRow } from "./mcp/McpRow";

export default function McpPage() {
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [editing, setEditing] = useState<McpServer | "new" | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [lastSync, setLastSync] = useState<McpSyncOutcome[] | null>(null);

  const list = useQuery({ queryKey: ["mcp_list"], queryFn: api.mcpList });
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["mcp_list"] });
  };

  const after = (outcomes: McpSyncOutcome[], prefix: string) => {
    setLastSync(outcomes);
    const bad = outcomes.filter((o) => !o.skipped && o.message.startsWith("同步失敗"));
    const written = outcomes.filter((o) => !o.skipped && o.written.length > 0);
    const skipped = outcomes.filter((o) => o.skipped);
    if (bad.length > 0) {
      setErr(`${prefix}，但同步失敗：${bad.map((b) => `${b.display}（${b.message}）`).join("；")}`);
      setMsg("");
    } else {
      setErr("");
      const parts = [
        written.length > 0
          ? `已寫入 ${written.map((w) => `${w.display} ${w.written.length} 項`).join("、")}`
          : "設定檔無變動",
        skipped.length > 0 ? `${skipped.map((s) => s.display).join("、")} 未安裝略過` : "",
      ].filter(Boolean);
      setMsg(`${prefix}：${parts.join("；")}`);
    }
    refresh();
  };

  const del = useMutation({
    mutationFn: (id: number) => api.mcpDelete(id),
    onSuccess: (o) => after(o, "已刪除"),
    onError: (e) => setErr(`刪除失敗：${String(e)}`),
  });
  const bind = useMutation({
    mutationFn: (v: { id: number; app: string; enabled: boolean }) =>
      api.mcpSetBinding(v.id, v.app, v.enabled),
    onSuccess: (o) => after(o, "已更新"),
    onError: (e) => setErr(`更新失敗：${String(e)}`),
  });
  const bindAll = useMutation({
    mutationFn: (v: { app: string; enabled: boolean }) =>
      api.mcpSetAppAll(v.app, v.enabled),
    onSuccess: (o) => after(o, "已整欄切換"),
    onError: (e) => setErr(`整欄切換失敗：${String(e)}`),
  });
  const sync = useMutation({
    mutationFn: api.mcpSync,
    onSuccess: (o) => after(o, "已重新同步"),
    onError: (e) => setErr(`同步失敗：${String(e)}`),
  });
  const imp = useMutation({
    mutationFn: api.mcpImport,
    onSuccess: (r) => {
      const failed = r.per_app.filter((p) => p.error);
      setMsg(
        `匯入完成：新增 ${r.added} 個、略過 ${r.skipped} 個` +
          (failed.length > 0 ? `；${failed.length} 個工具讀不到（見下方）` : ""),
      );
      setErr(failed.map((f) => `${f.app}：${f.error}`).join("；"));
      setLastSync(null);
      refresh();
    },
    onError: (e) => setErr(`匯入失敗：${String(e)}`),
  });

  const servers = list.data ?? [];
  const apps = useMemo(() => {
    const seen: { app: string; display: string }[] = [];
    for (const s of servers) {
      for (const b of s.bindings) {
        if (!seen.some((x) => x.app === b.app)) seen.push({ app: b.app, display: b.app });
      }
    }
    // 沒有伺服器時仍要顯示三個欄位（使用者才知道可以綁哪些工具）
    if (seen.length === 0) {
      return [
        { app: "claude", display: "Claude Code" },
        { app: "codex", display: "Codex" },
        { app: "opencode", display: "OpenCode" },
      ];
    }
    return seen;
  }, [servers]);

  const appLabel = (app: string) =>
    ({ claude: "Claude Code", codex: "Codex", opencode: "OpenCode" })[app] ?? app;

  return (
    <div className="space-y-4">
      {dialog}
      <div className="glass p-5">
        <div className="mb-3 flex flex-wrap items-center gap-2.5">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/70">
            <Icon name="cpu" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[15px] font-semibold tracking-tight text-fg">
              MCP 伺服器
            </div>
            <div className="text-[11px] text-fg/30">
              開啟某個工具＝把伺服器寫進它的設定檔；關閉＝從那個檔案移除。
              你自己手寫、沒匯入這裡的伺服器不會被動到。
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
              disabled={imp.isPending}
              onClick={() => imp.mutate()}
              title="讀遍 Claude Code／Codex／OpenCode 的設定檔，把伺服器收進來"
            >
              <Icon name="download" size={12} />
              {imp.isPending ? "匯入中…" : "匯入既有設定"}
            </button>
            <button
              className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
              disabled={sync.isPending}
              onClick={() => sync.mutate()}
              title="不改變任何設定，只把現況重新寫進設定檔"
            >
              <Icon name="refresh" size={12} />
              重新同步
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

        {msg && <p className="pb-2 text-xs text-emerald-400/80">{msg}</p>}
        {err && <p className="pb-2 text-xs break-words text-red-400">{err}</p>}

        {lastSync && lastSync.some((o) => o.backup) && (
          <p className="pb-2 text-[11px] break-all text-fg/30">
            改寫前已備份：
            {lastSync
              .filter((o) => o.backup)
              .map((o) => o.backup)
              .join("、")}
          </p>
        )}

        {list.isPending ? (
          <p className="text-sm text-fg/30">載入中…</p>
        ) : list.isError ? (
          <p className="text-sm text-fg/50">讀取失敗：{String(list.error)}</p>
        ) : servers.length === 0 ? (
          <p className="text-[13px] leading-relaxed text-fg/35">
            還沒有任何伺服器。按「新增」從樣板開始（fetch／time／memory／
            sequential-thinking／context7），或按「匯入既有設定」把你已經在工具裡
            設好的伺服器收進來。
          </p>
        ) : (
          <div className="space-y-2">
            {/* 標題列：每個工具一個「整欄開/關」 */}
            <div className="flex items-center gap-2 px-3 text-[11px] text-fg/35">
              <span className="min-w-0 flex-1">伺服器</span>
              {apps.map((a) => (
                <span key={a.app} className="flex w-[92px] shrink-0 items-center gap-1">
                  <span className="min-w-0 flex-1 truncate">{appLabel(a.app)}</span>
                  <button
                    className="shrink-0 rounded px-1 text-fg/30 hover:text-fg/80"
                    title={`${appLabel(a.app)}：全部開啟`}
                    onClick={() => bindAll.mutate({ app: a.app, enabled: true })}
                  >
                    ⏻
                  </button>
                  <button
                    className="shrink-0 rounded px-1 text-fg/30 hover:text-fg/80"
                    title={`${appLabel(a.app)}：全部關閉`}
                    onClick={() => bindAll.mutate({ app: a.app, enabled: false })}
                  >
                    ⭘
                  </button>
                </span>
              ))}
              <span className="w-[52px] shrink-0" />
            </div>
            {servers.map((s) => (
              <McpRow
                key={s.id}
                s={s}
                apps={apps.map((a) => a.app)}
                appLabel={appLabel}
                onToggle={(app, enabled) =>
                  bind.mutate({ id: s.id, app, enabled })
                }
                onEdit={() => setEditing(s)}
                onDelete={() =>
                  ask(`刪除 MCP 伺服器 ${s.name}？`, () => del.mutate(s.id), {
                    message:
                      "會先從所有已啟用的工具設定檔移除，再刪掉資料庫這一列。",
                  })
                }
              />
            ))}
          </div>
        )}

        <p className="pt-3 text-[11px] leading-relaxed text-fg/25">
          Claude Code 寫 <span className="font-mono">~/.claude.json</span> 的
          <span className="font-mono"> mcpServers</span>；Codex 寫
          <span className="font-mono"> ~/.codex/config.toml</span> 的
          <span className="font-mono"> [mcp_servers]</span>；OpenCode 寫
          <span className="font-mono"> opencode.json</span> 的
          <span className="font-mono"> mcp</span>。每次改寫前都會備份到
          <span className="font-mono"> backups/&lt;工具&gt;-mcp/</span>（保留 5 份）。
        </p>
      </div>

      {editing && (
        <McpForm
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={(outcomes) => {
            setEditing(null);
            after(outcomes, "已儲存");
          }}
        />
      )}
    </div>
  );
}
