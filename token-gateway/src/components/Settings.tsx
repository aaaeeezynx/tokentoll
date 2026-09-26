import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type UsageRange } from "../lib/api";
import { ACCENTS, applyAccentVars, isAccentId, loadAccent, saveAccentLocal } from "../lib/theme";
import { Icon } from "./icons";
import { RangePicker, type CustomRange } from "./RangePicker";
import { Toggle } from "./Toggle";

function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

function SectionHead(props: {
  icon: "switch" | "sliders" | "download" | "info";
  tile: string;
  title: string;
  caption?: string;
}) {
  return (
    <div className="mb-3 flex items-center gap-2.5">
      <span
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-white"
        style={{ background: props.tile }}
      >
        <Icon name={props.icon} size={17} />
      </span>
      <div className="min-w-0">
        <div className="text-[15px] font-semibold tracking-tight text-white">
          {props.title}
        </div>
        {props.caption && (
          <div className="truncate text-[11px] text-white/30">
            {props.caption}
          </div>
        )}
      </div>
    </div>
  );
}

export default function SettingsPage() {
  const qc = useQueryClient();
  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
  });
  const dir = useQuery({ queryKey: ["app_data_dir"], queryFn: api.appDataDir });

  const [port, setPort] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const [msg, setMsg] = useState("");
  const [accent, setAccent] = useState<string>(loadAccent());
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

  const pickAccent = (id: string) => {
    setAccent(id);
    applyAccentVars(id);
    saveAccentLocal(id);
    // 後端 SQLite 持久化（真相源）；失敗僅告警，下次啟動以後端值為準
    void api.settingsSetAccent(id).catch((e) => console.warn("accent persist failed:", e));
  };

  // 後端真相源同步（修復 localStorage 刷盤丟失導致的回退）
  useEffect(() => {
    const a = settings.data?.accent;
    if (a && isAccentId(a) && a !== accent) {
      setAccent(a);
      applyAccentVars(a);
      saveAccentLocal(a);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.data]);

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
      const blob = new Blob(["\uFEFF" + csv], {
        type: "text/csv;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const d = new Date();
      const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
      const rangeTag = csvCustom
        ? `custom-${csvCustom.start}-${csvCustom.end}`
        : csvRange;
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
          <p className="text-sm text-white/30">載入中…</p>
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
                <span className="text-xs text-white/40">
                  應用程式啟動時自動啟動網關
                </span>
              </div>
            </div>
            {msg && <p className="pt-1.5 text-xs text-white/50">{msg}</p>}
            <p className="pt-1.5 text-[11px] text-white/25">
              15721 常被 cc-switch 佔用，預設 15722。網關執行時不可改連接埠（需先停止）；改完後已切換工具需重新執行一次切換。
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

      {/* ── 外觀 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="sliders"
          tile="linear-gradient(160deg, #b48cff, #7c5cff)"
          title="外觀"
          caption="強調色即時套用全站"
        />
        <div className="mac-frow">
          <span className="mac-cap">強調色</span>
          <div className="flex items-center gap-2.5">
            {ACCENTS.map((a) => (
              <button
                key={a.id}
                title={a.label}
                onClick={() => pickAccent(a.id)}
                data-active={accent === a.id}
                className="accent-dot flex h-7 w-7 items-center justify-center rounded-full transition-transform hover:scale-110"
                style={{ background: a.value }}
              >
                {accent === a.id && (
                  <Icon name="check" size={13} className="text-white" />
                )}
              </button>
            ))}
            <span className="text-xs text-white/40">
              {ACCENTS.find((a) => a.id === accent)?.label}
            </span>
          </div>
        </div>
      </div>

      {/* ── 用量匯出 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="download"
          tile="linear-gradient(160deg, #34d399, #0a9e6e)"
          title="用量匯出"
          caption="請求明細 CSV（含 BOM，Excel 可直接開）"
        />
        <div className="mac-frow">
          <span className="mac-cap">範圍</span>
          <div className="flex flex-wrap items-center gap-2">
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
            <button
              className="btn-ghost flex shrink-0 items-center gap-1.5 px-3.5 py-1.5 text-[13px] disabled:opacity-40"
              disabled={exporting}
              onClick={() => void exportCsv()}
            >
              <Icon name="download" size={13} />
              {exporting ? "匯出中…" : "匯出 CSV"}
            </button>
          </div>
        </div>
        {csvMsg && <p className="pt-1.5 text-xs text-white/50">{csvMsg}</p>}
      </div>

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
            <p className="min-w-0 flex-1 font-mono text-[11px] break-all text-white/45">
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
