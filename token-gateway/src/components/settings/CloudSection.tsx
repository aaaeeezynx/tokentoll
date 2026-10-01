/**
 * 設定頁的「雲端同步」區塊（P4.10，對標 cc-switch 的 Cloud Sync → WebDAV）。
 *
 * 只有 WebDAV（S3 需要自簽 SigV4，我們沒有可驗證的端點 —— 矩陣上標明未做）。
 * 上傳會覆蓋遠端、下載會覆蓋本地；下載前會先顯示遠端快照，而且**下載前
 * 一定先自動備份本地資料庫**（與 cc-switch 相同）。
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useConfirm } from "../Confirm";
import { Toggle } from "../Toggle";
import { SectionHead } from "./SectionHead";
import { useI18n } from "../../lib/i18n";

function fmtTime(ms: number): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString();
}
function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / 1024 / 1024).toFixed(2)} MB`;
}

export function CloudSection() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [info, setInfo] = useState("");
  const [form, setForm] = useState({
    base_url: "",
    username: "",
    password: "",
    remote_root: "",
    profile: "",
  });
  const [loaded, setLoaded] = useState(false);

  const state = useQuery({ queryKey: ["cloud_state"], queryFn: api.cloudState });
  // 首次載入時把設定填進表單（密碼留空＝不變更）
  if (!loaded && state.data) {
    setLoaded(true);
    setForm({
      base_url: state.data.config.base_url,
      username: state.data.config.username,
      password: "",
      remote_root: state.data.config.remote_root,
      profile: state.data.config.profile,
    });
  }

  const refresh = () => void qc.invalidateQueries({ queryKey: ["cloud_state"] });
  const ok = (m: string) => {
    setMsg(m);
    setErr("");
    refresh();
  };
  const bad = (e: unknown) => setErr(String(e));

  const saveCfg = useMutation({
    mutationFn: () =>
      api.cloudSetConfig(
        form.base_url,
        form.username,
        form.password,
        form.remote_root,
        form.profile,
      ),
    onSuccess: () => {
      ok(t("settings.cloud.saved"));
      setForm((f) => ({ ...f, password: "" }));
      refresh();
    },
    onError: bad,
  });
  const test = useMutation({
    mutationFn: api.cloudTest,
    onSuccess: (m) => ok(m),
    onError: bad,
  });
  const upload = useMutation({
    mutationFn: api.cloudUpload,
    onSuccess: (o) => ok(`${o.note}`),
    onError: bad,
  });
  const remoteInfo = useMutation({
    mutationFn: api.cloudRemoteInfo,
    onSuccess: (s) =>
      setInfo(
        t("settings.cloud.snapshot", {
          protocol: s.protocol,
          schema: s.schema_version,
          app: s.app_version,
          time: fmtTime(s.timestamp_ms),
          size: fmtBytes(s.bytes),
        }) + (s.incompatible ? t("settings.cloud.incompatible") : ""),
      ),
    onError: bad,
  });
  const download = useMutation({
    mutationFn: api.cloudDownload,
    onSuccess: (o) => {
      ok(o.note);
      setInfo("");
    },
    onError: bad,
  });
  const autoSync = useMutation({
    mutationFn: (v: boolean) => api.cloudSetAuto(v),
    onSuccess: (s) => ok(t(s.auto_sync ? "settings.cloud.auto.on" : "settings.cloud.auto.off")),
    onError: bad,
  });

  const st = state.data;
  return (
    <div className="glass p-5">
      {dialog}
      <SectionHead
        icon="upload"
        tile="linear-gradient(160deg, #5AC8FA, #0A84FF)"
        title={t("settings.cloud.title")}
        caption={t("settings.cloud.caption")}
      />

      {(
        [
          ["base_url", t("settings.cloud.server"), t("settings.cloud.server.ph")],
          ["username", t("settings.cloud.user"), ""],
          [
            "password",
            t("settings.cloud.password"),
            t(
              st?.has_password
                ? "settings.cloud.password.keep"
                : "settings.cloud.password.ph",
            ),
          ],
          ["remote_root", t("settings.cloud.root"), t("settings.cloud.root.ph")],
          ["profile", t("settings.cloud.profile"), t("settings.cloud.profile.ph")],
        ] as const
      ).map(([key, label, ph]) => (
        <div className="mac-frow" key={key}>
          <span className="mac-cap">{label}</span>
          <input
            className="field w-full px-2.5 py-1.5 text-[12px]"
            type={key === "password" ? "password" : "text"}
            placeholder={ph}
            value={form[key]}
            onChange={(e) => setForm((f) => ({ ...f, [key]: e.target.value }))}
          />
        </div>
      ))}

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={saveCfg.isPending}
          onClick={() => saveCfg.mutate()}
        >
          {t("settings.cloud.save")}
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={test.isPending || !st?.config.base_url}
          onClick={() => test.mutate()}
        >
          {test.isPending ? t("settings.cloud.testing") : t("settings.cloud.test")}
        </button>
        <button
          className="btn-primary px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={upload.isPending || !st?.config.base_url}
          onClick={() =>
            ask(t("settings.cloud.upload.title"), () => upload.mutate(), {
              message: t("settings.cloud.upload.hint"),
              confirmLabel: t("settings.cloud.upload"),
            })
          }
        >
          {upload.isPending ? t("settings.cloud.uploading") : t("settings.cloud.upload")}
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={remoteInfo.isPending || !st?.config.base_url}
          onClick={() => remoteInfo.mutate()}
        >
          {t("settings.cloud.info")}
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={download.isPending || !st?.config.base_url}
          onClick={() =>
            ask(t("settings.cloud.download.title"), () => download.mutate(), {
              message:
                t("settings.cloud.download.hint"),
              confirmLabel: t("settings.cloud.download"),
            })
          }
        >
          {download.isPending ? t("settings.cloud.downloading") : t("settings.cloud.download")}
        </button>
        <label className="flex items-center gap-1.5 pl-1 text-[11px] text-fg/45">
          <Toggle
            checked={st?.auto_sync ?? false}
            onChange={(v) => autoSync.mutate(v)}
            size="sm"
            disabled={autoSync.isPending}
          />
          {t("settings.cloud.auto")}
        </label>
      </div>

      <p className="pt-2 text-[11px] text-fg/30">
        {t("settings.cloud.last", {
          time: fmtTime(st?.last_sync_ms ?? 0),
          result: st?.last_result ?? "",
        })}
        {st?.pending_changes ? t("settings.cloud.pending") : ""}
      </p>
      {info && <p className="pt-1 font-mono text-[11px] break-all text-fg/50">{info}</p>}
      {msg && <p className="pt-1 text-[11px] text-emerald-400/80">{msg}</p>}
      {err && <p className="pt-1 text-[11px] break-words text-red-400">{err}</p>}
    </div>
  );
}
