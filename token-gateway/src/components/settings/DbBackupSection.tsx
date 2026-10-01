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
import { useI18n } from "../../lib/i18n";

/** 小時 → 人話（走字典，所以英文介面也讀得懂）。 */
function intervalLabel(
  h: number,
  t: (k: string, vars?: Record<string, string | number>) => string,
): string {
  if (h <= 0) return t("settings.backup.interval.off");
  if (h === 168) return t("settings.backup.interval.days", { n: 7 });
  return t("settings.backup.interval.hours", { n: h });
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
  const { t } = useI18n();
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
      ok(t("settings.backup.scheduled"));
      refresh();
    },
    onError: bad,
  });
  const backupNow = useMutation({
    mutationFn: () => api.dbBackupNow(null),
    onSuccess: (b) => ok(t("settings.backup.created", { name: b.name, size: fmtBytes(b.bytes) })),
    onError: bad,
  });
  const restore = useMutation({
    mutationFn: (name: string) => api.dbBackupRestore(name),
    onSuccess: (o) =>
      ok(t("settings.backup.restore.msg", { name: o.safety_backup })),
    onError: bad,
  });
  const rename = useMutation({
    mutationFn: (v: { name: string; to: string }) => api.dbBackupRename(v.name, v.to),
    onSuccess: (b) => ok(t("settings.backup.renamed", { name: b.name })),
    onError: bad,
  });
  const del = useMutation({
    mutationFn: (name: string) => api.dbBackupDelete(name),
    onSuccess: () => ok(t("settings.backup.deleted")),
    onError: bad,
  });

  const backups = state.data?.backups ?? [];

  return (
    <div className="glass p-5">
      {dialog}
      <SectionHead
        icon="download"
        tile="linear-gradient(160deg, #34d399, #0a9e6e)"
        title={t("settings.backup.title")}
        caption={t("settings.backup.caption")}
      />

      <div className="mac-frow">
        <span className="mac-cap">{t("settings.backup.auto")}</span>
        <div className="flex flex-wrap items-center gap-2">
          <div className="w-40">
            <PopSelect
              value={String(state.data?.interval_hours ?? 24)}
              onChange={(v) => schedule.mutate({ interval: Number(v) })}
              options={(state.data?.interval_options ?? [0, 6, 12, 24, 48, 168]).map((h) => ({
                value: String(h),
                label: intervalLabel(h, t),
              }))}
            />
          </div>
          <span className="text-[11px] text-fg/35">{t("settings.backup.keep")}</span>
          <div className="w-28">
            <PopSelect
              value={String(state.data?.retention ?? 10)}
              onChange={(v) => schedule.mutate({ retention: Number(v) })}
              options={(state.data?.retention_options ?? [3, 5, 10, 15, 20, 30, 50]).map((k) => ({
                value: String(k),
                label: t("settings.backup.keep.n", { n: k }),
              }))}
            />
          </div>
          <button
            className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
            disabled={backupNow.isPending}
            onClick={() => backupNow.mutate()}
          >
            {backupNow.isPending ? t("settings.backup.working") : t("settings.backup.now")}
          </button>
        </div>
      </div>
      <p className="text-[11px] text-fg/25">
        {t("settings.backup.last", {
          time: fmtTime(state.data?.last_ms ?? 0),
          dir: state.data?.dir ?? "",
        })}
      </p>

      {msg && <p className="pt-1.5 text-[11px] text-emerald-400/80">{msg}</p>}
      {err && <p className="pt-1.5 text-[11px] break-words text-red-400">{err}</p>}

      {state.isPending ? (
        <p className="pt-2 text-[13px] text-fg/30">{t("common.loading")}</p>
      ) : backups.length === 0 ? (
        <p className="pt-2 text-[13px] text-fg/35">{t("settings.backup.empty")}</p>
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
                title={t("settings.backup.restore.tip")}
                onClick={() =>
                  ask(t("settings.backup.restore.title", { name: b.name }), () => restore.mutate(b.name), {
                    message:
                      t("settings.backup.restore.hint"),
                    confirmLabel: t("settings.backup.restore"),
                  })
                }
              >
                {t("settings.backup.restore")}
              </button>
              <button
                className="btn-ghost px-2.5 py-1 text-[11px]"
                onClick={() => {
                  const to = window.prompt(t("settings.backup.rename.ph"), b.name);
                  if (to && to !== b.name) rename.mutate({ name: b.name, to });
                }}
              >
                {t("settings.backup.rename")}
              </button>
              <button
                className="btn-ghost px-2.5 py-1 text-[11px] text-red-400/80"
                onClick={() =>
                  ask(t("settings.backup.delete.title", { name: b.name }), () => del.mutate(b.name), {
                    message: t("settings.backup.delete.hint"),
                  })
                }
              >
                {t("settings.backup.delete")}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
