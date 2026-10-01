/**
 * 設定頁的「更新」區塊（P4.9，對標 cc-switch 的 About → 檢查更新）。
 *
 * **我們只做「檢查」不做自動安裝**：Tauri 的自動安裝需要簽章過的更新檔 ＋
 * 對應公鑰，而這個 App 還沒有發佈管道。畫面上把這件事講清楚，
 * 並給出下載連結（用既有的 opener 外掛開啟瀏覽器）。
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type UpdateCheck } from "../../lib/api";
import { SectionHead } from "./SectionHead";
import { useI18n } from "../../lib/i18n";

function fmtTime(ms: number): string {
  return ms ? new Date(ms).toLocaleString() : "—";
}

export function UpdateSection() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [url, setUrl] = useState<string | null>(null);
  const [result, setResult] = useState<UpdateCheck | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  const state = useQuery({ queryKey: ["update_state"], queryFn: api.updateState });
  const shownUrl = url ?? state.data?.manifest_url ?? "";

  const save = useMutation({
    mutationFn: (v: string) => api.updateSetUrl(v),
    onSuccess: () => {
      setMsg("已儲存更新資訊網址");
      setErr("");
      void qc.invalidateQueries({ queryKey: ["update_state"] });
    },
    onError: (e) => setErr(String(e)),
  });
  const check = useMutation({
    mutationFn: api.updateCheck,
    onSuccess: (r) => {
      setResult(r);
      setMsg(r.message);
      setErr("");
      void qc.invalidateQueries({ queryKey: ["update_state"] });
    },
    onError: (e) => {
      setErr(String(e));
      setResult(null);
    },
  });

  return (
    <div className="glass p-5">
      <SectionHead
        icon="refresh"
        tile="linear-gradient(160deg, #64D2FF, #0A84FF)"
        title={t("settings.update.title")}
        caption={t("settings.update.caption")}
      />

      <div className="mac-frow">
        <span className="mac-cap">{t("settings.update.url")}</span>
        <input
          className="field w-full px-2.5 py-1.5 text-[12px] font-mono"
          placeholder="https://…/latest.json"
          value={shownUrl}
          onChange={(e) => setUrl(e.target.value)}
        />
      </div>
      <div className="flex flex-wrap items-center gap-2 pt-1">
        <button
          className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={save.isPending}
          onClick={() => save.mutate(shownUrl)}
        >
          {t("settings.update.save")}
        </button>
        <button
          className="btn-primary px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={check.isPending || !shownUrl}
          onClick={() => check.mutate()}
        >
          {check.isPending ? t("settings.update.checking") : t("settings.update.check")}
        </button>
        {state.data && (
          <span className="text-[11px] text-fg/35">
            {t("settings.update.current", { v: state.data.current })}｜上次檢查：
            {fmtTime(state.data.last_check_ms)}
            {state.data.last_result ? `（${state.data.last_result}）` : ""}
          </span>
        )}
      </div>

      {result && (
        <div className="mt-2 space-y-1.5 rounded-lg bg-fg/[0.04] px-3 py-2.5 text-[12px]">
          <div className="text-fg/80">
            {result.has_update
              ? t("settings.update.found", { v: result.latest })
              : t("settings.update.latest")}
          </div>
          {result.pub_date && (
            <div className="text-[11px] text-fg/35">
              {t("settings.update.date")} {result.pub_date}
            </div>
          )}
          {result.notes && (
            <pre className="max-h-40 overflow-auto text-[11px] whitespace-pre-wrap text-fg/55">
              {result.notes}
            </pre>
          )}
          {result.url && (
            <button
              className="btn-ghost px-2.5 py-1 text-[11px]"
              onClick={() => void openUrl(result.url)}
            >
              {t("settings.update.download")}
            </button>
          )}
          {!result.signed && (
            <p className="text-[11px] text-amber-300/80">
              {t("settings.update.no_auto")}
            </p>
          )}
        </div>
      )}

      {msg && <p className="pt-1.5 text-[11px] text-emerald-400/80">{msg}</p>}
      {err && <p className="pt-1.5 text-[11px] break-words text-red-400">{err}</p>}
    </div>
  );
}
