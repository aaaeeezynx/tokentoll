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
import { AppearanceSection } from "./settings/AppearanceSection";
import { DeeplinkSection } from "./settings/DeeplinkSection";
import { DbBackupSection } from "./settings/DbBackupSection";
import { CloudSection } from "./settings/CloudSection";

function copyText(t: string) {
  void navigator.clipboard?.writeText(t);
}

export default function SettingsPage() {
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
    onSuccess: () => setMsg(`連接埠 ${port} 可用`),
    onError: (e) => setMsg(String(e)),
  });
  const save = useMutation({
    mutationFn: () => api.settingsSet(Number(port), autoStart),
    onSuccess: (s) => {
      setMsg(`已儲存：連接埠 ${s.gateway_port}，開機自啟${s.auto_start_proxy ? "開" : "關"}`);
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
      setCsvMsg(`已匯出 ${rows} 筆`);
    } catch (e) {
      setCsvMsg(`匯出失敗：${String(e)}`);
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
          title="網關"
          caption="本機代理監聽（僅 127.0.0.1）"
        />
        {settings.isPending ? (
          <p className="text-sm text-fg/30">載入中…</p>
        ) : (
          <div>
            <div className="mac-frow">
              <span className="mac-cap">監聽連接埠</span>
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
                  檢測
                </button>
              </div>
            </div>
            <div className="mac-frow">
              <span className="mac-cap">自動啟動</span>
              <div className="flex items-center gap-2">
                <Toggle checked={autoStart} onChange={setAutoStart} size="sm" />
                <span className="text-xs text-fg/40">應用程式啟動時自動啟動網關</span>
              </div>
            </div>
            {msg && <p className="pt-1.5 text-xs text-fg/50">{msg}</p>}
            <p className="pt-1.5 text-[11px] text-fg/25">
              15721 常被 cc-switch 佔用，預設 15722。網關執行時不可改連接埠（需先停止）；
              改完後已切換工具需重新執行一次切換。
            </p>
            <div className="flex justify-end pt-2.5">
              <button
                className="btn-primary px-5 py-2 text-sm disabled:opacity-40"
                disabled={save.isPending}
                onClick={() => save.mutate()}
              >
                儲存
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
          title="用量匯出"
          caption="請求明細 CSV（含 BOM，Excel 可直接開）"
        />
        <div className="mac-frow">
          <span className="mac-cap">區間</span>
          <RangePicker
            presets={[
              { id: "today", label: "今日" },
              { id: "7d", label: "最近 7 天" },
              { id: "30d", label: "最近 30 天" },
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
            {exporting ? "匯出中…" : "匯出 CSV"}
          </button>
        </div>
        {csvMsg && <p className="pt-1.5 text-xs text-fg/50">{csvMsg}</p>}
      </div>

      {/* ── 資料庫備份（P4.8） ── */}
      <DbBackupSection />

      {/* ── 雲端同步（P4.10） ── */}
      <CloudSection />

      {/* ── Deep Link 協定（P4.1） ── */}
      <DeeplinkSection />

      {/* ── 關於 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="info"
          tile="linear-gradient(160deg, #8e8e93, #48484e)"
          title="關於"
          caption="Token Gateway v2 · Tauri 本地網關"
        />
        <div className="mac-frow">
          <span className="mac-cap">數據目錄</span>
          <div className="flex items-center gap-2">
            <p className="min-w-0 flex-1 font-mono text-[11px] break-all text-fg/45">
              {dir.isPending ? "讀取中…" : dir.isError ? "讀取失敗" : dir.data}
            </p>
            {!dir.isPending && !dir.isError && dir.data && (
              <button
                className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1 text-xs"
                onClick={() => copyText(dir.data as string)}
              >
                <Icon name="copy" size={12} />
                複製
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
