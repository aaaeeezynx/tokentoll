/**
 * 設定頁的「資料庫備份」區塊（P4.8，對標 cc-switch 的 Backup Management）。
 *
 * 自動備份間隔／保留份數、立即備份、備份清單（還原／改名／刪除）。
 * 還原一定先自動做一份安全備份（cc-switch 也是），所以按下去不怕後悔。
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useConfirm } from "../Confirm";
import { PopSelect } from "../PopSelect";
import { SectionHead } from "./SectionHead";

/** 小時 → 人話。 */
function intervalLabel(h: number): string {
  if (h <= 0) return "停用";
  if (h === 168) return "每 7 天";
  return `每 ${h} 小時`;
}

function fmtTime(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / 1024 / 1024).toFixed(2)} MB`;
}

export function DbBackupSection() {
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const state = useQuery({ queryKey: ["db_backup"], queryFn: api.dbBackupState });

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["db_backup"] });
  };
  const ok = (m: string) => {
    setMsg(m);
    setErr("");
    refresh();
  };
  const bad = (e: unknown) => setErr(String(e));

  const schedule = useMutation({
    mutationFn: (v: { interval?: number; retention?: number }) =>
      api.dbBackupSetSchedule(v.interval, v.retention),
    onSuccess: () => {
      ok("已更新備份排程");
      refresh();
    },
    onError: bad,
  });
  const backupNow = useMutation({
    mutationFn: () => api.dbBackupNow(null),
    onSuccess: (b) => ok(`已建立備份 ${b.name}（${fmtBytes(b.bytes)}）`),
    onError: bad,
  });
  const restore = useMutation({
    mutationFn: (name: string) => api.dbBackupRestore(name),
    onSuccess: (o) => ok(`${o.note}；安全備份 ${o.safety_backup}`),
    onError: bad,
  });
  const rename = useMutation({
    mutationFn: (v: { name: string; to: string }) => api.dbBackupRename(v.name, v.to),
    onSuccess: (b) => ok(`已改名為 ${b.name}`),
    onError: bad,
  });
  const del = useMutation({
    mutationFn: (name: string) => api.dbBackupDelete(name),
    onSuccess: () => ok("已刪除備份"),
    onError: bad,
  });

  const backups = state.data?.backups ?? [];

  return (
    <div className="glass p-5">
      {dialog}
      <SectionHead
        icon="download"
        tile="linear-gradient(160deg, #34d399, #0a9e6e)"
        title="資料庫備份"
        caption="自動備份排程與還原（還原前會先自動備份當前資料庫）"
      />

      <div className="mac-frow">
        <span className="mac-cap">自動備份</span>
        <div className="flex flex-wrap items-center gap-2">
          <div className="w-40">
            <PopSelect
              value={String(state.data?.interval_hours ?? 24)}
              onChange={(v) => schedule.mutate({ interval: Number(v) })}
              options={(state.data?.interval_options ?? [0, 6, 12, 24, 48, 168]).map((h) => ({
                value: String(h),
                label: intervalLabel(h),
              }))}
            />
          </div>
          <span className="text-[11px] text-fg/35">保留</span>
          <div className="w-28">
            <PopSelect
              value={String(state.data?.retention ?? 10)}
              onChange={(v) => schedule.mutate({ retention: Number(v) })}
              options={(state.data?.retention_options ?? [3, 5, 10, 15, 20, 30, 50]).map((k) => ({
                value: String(k),
                label: `${k} 份`,
              }))}
            />
          </div>
          <button
            className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
            disabled={backupNow.isPending}
            onClick={() => backupNow.mutate()}
          >
            {backupNow.isPending ? "備份中…" : "立即備份"}
          </button>
        </div>
      </div>
      <p className="text-[11px] text-fg/25">
        上次備份：{fmtTime(state.data?.last_ms ?? 0)}；目錄：
        <span className="font-mono">{state.data?.dir ?? ""}</span>
        （超過保留份數會自動刪掉最舊的）
      </p>

      {msg && <p className="pt-1.5 text-[11px] text-emerald-400/80">{msg}</p>}
      {err && <p className="pt-1.5 text-[11px] break-words text-red-400">{err}</p>}

      {state.isPending ? (
        <p className="pt-2 text-[13px] text-fg/30">讀取中…</p>
      ) : backups.length === 0 ? (
        <p className="pt-2 text-[13px] text-fg/35">
          還沒有備份 —— 按「立即備份」建立第一份。
        </p>
      ) : (
        <div className="space-y-1.5 pt-2.5">
          {backups.map((b) => (
            <div
              key={b.name}
              className="flex flex-wrap items-center gap-2 rounded-lg bg-fg/[0.04] px-3 py-2"
              data-backup={b.name}
            >
              <span className="text-[13px] text-fg/80">{b.name}</span>
              <span className="font-mono text-[10px] text-fg/30">{fmtTime(b.created_ms)}</span>
              <span className="text-[11px] text-fg/30">{fmtBytes(b.bytes)}</span>
              <span className="flex-1" />
              <button
                className="btn-ghost px-2.5 py-1 text-[11px] disabled:opacity-40"
                disabled={restore.isPending}
                title="還原這份備份（會先自動備份當前資料庫）"
                onClick={() =>
                  ask(`還原備份「${b.name}」？`, () => restore.mutate(b.name), {
                    message:
                      "目前的資料會被這份備份覆蓋。系統會先自動建立一份安全備份，所以後悔還救得回來。",
                    confirmLabel: "還原",
                  })
                }
              >
                還原
              </button>
              <button
                className="btn-ghost px-2.5 py-1 text-[11px]"
                onClick={() => {
                  const to = window.prompt("新的備份名稱", b.name);
                  if (to && to !== b.name) rename.mutate({ name: b.name, to });
                }}
              >
                改名
              </button>
              <button
                className="btn-ghost px-2.5 py-1 text-[11px] text-red-400/80"
                onClick={() =>
                  ask(`刪除備份「${b.name}」？`, () => del.mutate(b.name), {
                    message: "刪除後無法復原（不影響目前資料庫）。",
                  })
                }
              >
                刪除
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
