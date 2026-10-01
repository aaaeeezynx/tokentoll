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
      ok("已儲存雲端同步設定");
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
        `遠端快照：協定 ${s.protocol}｜schema v${s.schema_version}｜App ${s.app_version}｜` +
          `${fmtTime(s.timestamp_ms)}｜${fmtBytes(s.bytes)}` +
          (s.incompatible ? "（⚠ 協定版本不符）" : ""),
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
    onSuccess: (s) => ok(s.auto_sync ? "已開啟自動同步" : "已關閉自動同步"),
    onError: bad,
  });

  const st = state.data;
  return (
    <div className="glass p-5">
      {dialog}
      <SectionHead
        icon="upload"
        tile="linear-gradient(160deg, #5AC8FA, #0A84FF)"
        title="雲端同步"
        caption="用 WebDAV 在裝置之間同步設定（同時只能開一種）"
      />

      {(
        [
          ["base_url", "伺服器網址（例如 https://dav.example.com/remote.php/dav/files/me）"],
          ["username", "帳號"],
          [
            "password",
            st?.has_password ? "密碼（留空＝不變更）" : "密碼／應用程式密碼",
          ],
          ["remote_root", "遠端根目錄（預設 token-gateway-sync）"],
          ["profile", "設定檔名稱（預設 default）"],
        ] as const
      ).map(([key, ph]) => (
        <div className="mac-frow" key={key}>
          <span className="mac-cap">
            {key === "base_url" ? "伺服器" : key === "username" ? "帳號" : key === "password" ? "密碼" : key === "remote_root" ? "遠端目錄" : "設定檔"}
          </span>
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
          儲存設定
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={test.isPending || !st?.config.base_url}
          onClick={() => test.mutate()}
        >
          {test.isPending ? "測試中…" : "測試連線"}
        </button>
        <button
          className="btn-primary px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={upload.isPending || !st?.config.base_url}
          onClick={() =>
            ask("上傳本地資料庫到雲端？", () => upload.mutate(), {
              message: "上傳會**覆蓋遠端**的舊資料。本地不受影響。",
              confirmLabel: "上傳",
            })
          }
        >
          {upload.isPending ? "上傳中…" : "上傳"}
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={remoteInfo.isPending || !st?.config.base_url}
          onClick={() => remoteInfo.mutate()}
        >
          看遠端快照
        </button>
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={download.isPending || !st?.config.base_url}
          onClick={() =>
            ask("從雲端下載並覆蓋本地資料庫？", () => download.mutate(), {
              message:
                "下載會**覆蓋本地**（遠端優先）。下載前會先自動備份本地資料庫，所以後悔還救得回來。",
              confirmLabel: "下載",
            })
          }
        >
          {download.isPending ? "下載中…" : "下載"}
        </button>
        <label className="flex items-center gap-1.5 pl-1 text-[11px] text-fg/45">
          <Toggle
            checked={st?.auto_sync ?? false}
            onChange={(v) => autoSync.mutate(v)}
            size="sm"
            disabled={autoSync.isPending}
          />
          自動同步
        </label>
      </div>

      <p className="pt-2 text-[11px] text-fg/30">
        上次同步：{fmtTime(st?.last_sync_ms ?? 0)}
        {st?.last_result ? `｜${st.last_result}` : ""}
        {st?.pending_changes ? "｜（本地有變更，尚未上傳）" : ""}
      </p>
      {info && <p className="pt-1 font-mono text-[11px] break-all text-fg/50">{info}</p>}
      {msg && <p className="pt-1 text-[11px] text-emerald-400/80">{msg}</p>}
      {err && <p className="pt-1 text-[11px] break-words text-red-400">{err}</p>}
    </div>
  );
}
