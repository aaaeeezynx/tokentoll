/**
 * 設定頁。
 *
 * 這一頁只留「版面 ＋ 網關 ＋ 用量匯出 ＋ 關於」，其餘區塊各自住在
 * `components/settings/` 底下（外觀／Deep Link／資料庫備份）—— 這支檔案原本
 * 因為一路加功能長到 487 行，超過專案的 400 行上限，所以把整塊功能拆出去。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type UsageRange } from "../lib/api";
import { Icon } from "./icons";
import { RangePicker, type CustomRange } from "./RangePicker";
import { Toggle } from "./Toggle";
import { SectionHead } from "./settings/SectionHead";
import { useI18n } from "../lib/i18n";
import { AppearanceSection } from "./settings/AppearanceSection";
import { DeeplinkSection } from "./settings/DeeplinkSection";
import { DbBackupSection } from "./settings/DbBackupSection";
import { CloudSection } from "./settings/CloudSection";
import { UpdateSection } from "./settings/UpdateSection";

function copyText(t: string) {
  void navigator.clipboard?.writeText(t);
}

export default function SettingsPage() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settingsGet });
  const dir = useQuery({ queryKey: ["app_data_dir"], queryFn: api.appDataDir });

  const [port, setPort] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const [msg, setMsg] = useState("");
  const [csvRange, setCsvRange] = useState<UsageRange>("30d");
  const [csvCustom, setCsvCustom] = useState<CustomRange | null>(null);
  const [csvMsg, setCsvMsg] = useState("");
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    if (settings.data) {
      setPort(String(settings.data.gateway_port));
      setAutoStart(settings.data.auto_start_proxy);
    }
  }, [settings.data]);

  const check = useMutation({
    mutationFn: () => api.proxyCheckPort(Number(port)),
    onSuccess: () => setMsg(t("settings.gateway.ok", { port })),
    onError: (e) => setMsg(String(e)),
  });
  const save = useMutation({
    mutationFn: () => api.settingsSet(Number(port), autoStart),
    onSuccess: (s) => {
      setMsg(
        t("settings.gateway.saved", {
          port: s.gateway_port,
          auto: t(s.auto_start_proxy ? "settings.gateway.on" : "settings.gateway.off"),
        }),
      );
      void qc.invalidateQueries({ queryKey: ["settings"] });
      void qc.invalidateQueries({ queryKey: ["tools"] });
    },
    onError: (e) => setMsg(String(e)),
  });

  const exportCsv = async () => {
    setExporting(true);
    setCsvMsg("");
    try {
      const csv = await api.usageExportCsv(
        csvRange,
        null,
        null,
        null,
        csvCustom?.start ?? null,
        csvCustom?.end ?? null,
      );
      const blob = new Blob(["\uFEFF" + csv], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const d = new Date();
      const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
      const rangeTag = csvCustom ? `custom-${csvCustom.start}-${csvCustom.end}` : csvRange;
      const a = document.createElement("a");
      a.href = url;
      a.download = `token-gateway-usage-${rangeTag}-${stamp}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      const rows = csv.trim().split("\n").length - 1;
      setCsvMsg(t("settings.export.done", { n: rows }));
    } catch (e) {
      setCsvMsg(t("settings.export.failed", { e: String(e) }));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* ── 網關 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="switch"
          tile="linear-gradient(160deg, #3395ff, #0a64d8)"
          title={t("settings.gateway.title")}
          caption={t("settings.gateway.caption")}
        />
        {settings.isPending ? (
          <p className="text-sm text-fg/30">{t("common.loading")}</p>
        ) : (
          <div>
            <div className="mac-frow">
              <span className="mac-cap">{t("settings.gateway.port")}</span>
              <div className="flex items-center gap-2">
                <input
                  className="field w-full px-3 py-1.5 text-[13px]"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                />
                <button
                  className="btn-ghost shrink-0 px-3.5 py-1.5 text-[13px]"
                  onClick={() => check.mutate()}
                >
                  {t("settings.gateway.check")}
                </button>
              </div>
            </div>
            <div className="mac-frow">
              <span className="mac-cap">{t("settings.gateway.autostart")}</span>
              <div className="flex items-center gap-2">
                <Toggle checked={autoStart} onChange={setAutoStart} size="sm" />
                <span className="text-xs text-fg/40">{t("settings.gateway.autostart.hint")}</span>
              </div>
            </div>
            {msg && <p className="pt-1.5 text-xs text-fg/50">{msg}</p>}
            <p className="pt-1.5 text-[11px] text-fg/25">
              {t("settings.gateway.hint")}
            </p>
            <div className="flex justify-end pt-2.5">
              <button
                className="btn-primary px-5 py-2 text-sm disabled:opacity-40"
                disabled={save.isPending}
                onClick={() => save.mutate()}
              >
                {t("settings.gateway.save")}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── 外觀（P4.3／P4.4／P4.7） ── */}
      <AppearanceSection />

      {/* ── 用量匯出 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="download"
          tile="linear-gradient(160deg, #34d399, #0a9e6e)"
          title={t("settings.export.title")}
          caption={t("settings.export.caption")}
        />
        <div className="mac-frow">
          <span className="mac-cap">{t("settings.export.range")}</span>
          <RangePicker
            presets={[
              { id: "today", label: t("settings.export.range.today") },
              { id: "7d", label: t("settings.export.range.7d") },
              { id: "30d", label: t("settings.export.range.30d") },
            ]}
            range={csvRange}
            custom={csvCustom}
            onPreset={(v) => setCsvRange(v as UsageRange)}
            onCustom={setCsvCustom}
            btnClassName="px-2.5 py-1 text-xs font-medium"
          />
        </div>
        <div className="flex justify-end pt-2.5">
          <button
            className="btn-primary px-5 py-2 text-sm disabled:opacity-40"
            disabled={exporting}
            onClick={() => void exportCsv()}
          >
            {exporting ? t("settings.export.working") : t("settings.export.csv")}
          </button>
        </div>
        {csvMsg && <p className="pt-1.5 text-xs text-fg/50">{csvMsg}</p>}
      </div>

      {/* ── 資料庫備份（P4.8） ── */}
      <DbBackupSection />

      {/* ── 更新（P4.9） ── */}
      <UpdateSection />

      {/* ── 雲端同步（P4.10） ── */}
      <CloudSection />

      {/* ── Deep Link 協定（P4.1） ── */}
      <DeeplinkSection />

      {/* ── 關於 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="info"
          tile="linear-gradient(160deg, #8e8e93, #48484e)"
          title={t("settings.about.title")}
          caption={t("settings.about.caption")}
        />
        <div className="mac-frow">
          <span className="mac-cap">{t("settings.about.datadir")}</span>
          <div className="flex items-center gap-2">
            <p className="min-w-0 flex-1 font-mono text-[11px] break-all text-fg/45">
              {dir.isPending ? t("common.loading") : dir.isError ? t("common.readFailed") : dir.data}
            </p>
            {!dir.isPending && !dir.isError && dir.data && (
              <button
                className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1 text-xs"
                onClick={() => copyText(dir.data as string)}
              >
                <Icon name="copy" size={12} />
                {t("settings.about.copy")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
