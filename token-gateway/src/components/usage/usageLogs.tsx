//! 歷史回填對話框與「最近請求」清單。
//!
//! 由 `usageCharts.tsx` 拆出（該檔原本同時放圖表元件與這兩個日誌元件，
//! 2026-09-28 時 546 行）。程式碼語意未改，只搬位置。

import { api, type HistoryScanTool } from "../../lib/api";
import { Icon } from "../icons";
import { Logo, appLabel } from "../logos";
import { fmtInt } from "./usageTypes";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

// `fmtBytes` 與 `fmtTime` 仍住在 `usageCharts.tsx`（那裡是純格式化／圖表核心）。
import { fmtBytes, fmtTime } from "./usageCharts";

export function HistoryImportDialog(props: { onClose: () => void }) {
  const qc = useQueryClient();
  const [checked, setChecked] = useState<Record<string, boolean>>({
    claude: true,
    codex: true,
    opencode: true,
  });
  const [result, setResult] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const scan = useQuery({
    queryKey: ["history_scan"],
    queryFn: api.historyScan,
  });
  const tools: HistoryScanTool[] = scan.data || [];

  const toggle = (t: string) =>
    setChecked((c) => ({ ...c, [t]: !c[t] }));

  const run = async () => {
    const sel = tools
      .map((t) => t.tool)
      .filter((t) => checked[t] !== false);
    if (sel.length === 0 || running) return;
    setRunning(true);
    setResult(null);
    try {
      const s = await api.historyImport(sel);
      const per = s.per_tool
        .map(
          (p) =>
            `${p.tool}: ${p.rows} 筆 / ${fmtInt(p.tokens)} tokens / $${p.cost_usd.toFixed(4)}`,
        )
        .join("；");
      const unpriced =
        s.unpriced_top.length > 0
          ? `未定價模型（費用記 0，可到定價頁補）：${s.unpriced_top
              .map((u) => `${u.model}(${fmtInt(u.tokens)})`)
              .join("、")}`
          : "全部命中定價";
      setResult(
        `寫入 ${s.rows_added} 筆（${per}）。去重跳過 ${s.dupes_skipped}，空行跳過 ${s.empty_skipped}。${unpriced}`,
      );
      void qc.invalidateQueries({ queryKey: ["usage_summary"] });
      void qc.invalidateQueries({ queryKey: ["usage_trend"] });
      void qc.invalidateQueries({ queryKey: ["usage_trend_by_app"] });
      void qc.invalidateQueries({ queryKey: ["usage_by_provider"] });
      void qc.invalidateQueries({ queryKey: ["usage_by_model"] });
      void qc.invalidateQueries({ queryKey: ["usage_hourly"] });
      void qc.invalidateQueries({ queryKey: ["recent_logs"] });
    } catch (e) {
      setResult(`回填失敗：${String(e)}`);
    } finally {
      setRunning(false);
    }
  };

  const [closing, setClosing] = useState(false);
  const requestClose = () => {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => props.onClose(), 200);
  };

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div className={`w-full max-w-lg glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}>
        <div className="mb-1 text-sm font-medium text-white/80">歷史回填</div>
        <p className="mb-3 text-xs text-white/30">
          離線解析本機會話（Claude / Codex / OpenCode），寫入用量統計。
          已回填過的不重複；與網關日誌疑似重複的自動跳過；費用按當前定價快照計算。
        </p>
        {scan.isPending && (
          <p className="text-sm text-white/30">掃描本機文件中…</p>
        )}
        {scan.isError && (
          <p className="text-sm text-red-400">掃描失敗</p>
        )}
        {tools.map((t) => (
          <label
            key={t.tool}
            className="flex cursor-pointer items-center gap-2 py-1.5 text-sm text-white/70"
          >
            <input
              type="checkbox"
              checked={checked[t.tool] !== false}
              disabled={t.files === 0}
              onChange={() => toggle(t.tool)}
            />
            <span className="w-20">{t.tool}</span>
            <span className="text-xs text-white/30">
              {t.files === 0
                ? "無數據"
                : `${t.files} 文件 · ${fmtBytes(t.bytes)}${t.sessions > 0 ? ` · ${t.sessions} 會話` : ""}`}
            </span>
          </label>
        ))}
        {result && (
          <p className="mt-3 rounded-xl bg-black/20 p-2.5 text-xs leading-relaxed text-white/60">
            {result}
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            className="btn-ghost px-4 py-2 text-sm"
            onClick={requestClose}
          >
            關閉
          </button>
          <button
            className="btn-primary px-4 py-2 text-sm"
            disabled={running || scan.isPending}
            onClick={() => void run()}
          >
            {running ? "回填中…" : "開始回填"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── 最近請求 ──────────────────────────────────────────────────────────────

export function timeAgo(ts: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (s < 60) return `${s} 秒前更新`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分鐘前更新`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小時前更新`;
  return `${Math.floor(h / 24)} 天前更新`;
}

export function RecentLogs() {
  const qc = useQueryClient();
  const logs = useQuery({
    queryKey: ["recent_logs"],
    queryFn: () => api.recentLogs(20),
    refetchInterval: 5000,
  });
  const refreshAll = () => {
    for (const k of [
      "recent_logs",
      "usage_summary",
      "usage_trend",
      "usage_trend_by_app",
      "usage_by_provider",
      "usage_by_model",
      "usage_hourly",
    ])
      void qc.invalidateQueries({ queryKey: [k] });
  };
  if (logs.isPending)
    return <p className="text-sm text-white/30">載入中…</p>;
  if (logs.isError)
    return <p className="text-sm text-red-400">日誌載入失敗</p>;
  if (logs.data.length === 0)
    return (
      <p className="text-sm text-white/20">
        暫無請求。啟動網關並把工具 base_url 指向它後，這裡會即時記錄。
      </p>
    );
  const newest = logs.data[0]?.ts ?? 0;
  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <div className="text-sm font-semibold tracking-tight text-white/80">
          最近請求
        </div>
        {newest > 0 && (
          <span className="text-[11px] text-white/25">{timeAgo(newest)}</span>
        )}
        <span className="flex-1" />
        <button
          className="btn-ghost px-2.5 py-1 text-xs"
          onClick={refreshAll}
          title="立即重新整理全部用量"
        >
          重新整理
        </button>
      </div>
    <div className="overflow-x-auto">
      <table className="w-full min-w-[600px] text-left text-xs">
        <thead>
          <tr className="text-white/55">
            <th className="py-1.5 pr-2 whitespace-nowrap">時間</th>
            <th className="py-1.5 pr-2 whitespace-nowrap">本機工具</th>
            <th className="py-1.5 pr-2">模型</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">輸入</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">輸出</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">費用$</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">狀態</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">來源</th>
          </tr>
        </thead>
        <tbody>
          {logs.data.map((l) => (
            <tr key={l.id} className="border-t border-white/[0.04]">
              <td className="py-2 pr-2 font-mono whitespace-nowrap text-white/30">
                {fmtTime(l.ts)}
              </td>
              <td className="py-2 pr-2 whitespace-nowrap text-white/60">
                <span className="inline-flex items-center gap-1.5">
                  <Logo name={l.app} size={13} className="shrink-0 opacity-80" />
                  {appLabel(l.app)}
                  {l.is_stream ? (
                    <Icon name="bolt-fill" size={11} className="text-[#ffd60a]" />
                  ) : null}
                </span>
              </td>
              <td className="max-w-40 truncate py-2 pr-2 text-white/60">
                {l.model_raw || "—"}
              </td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">
                {l.in_tok.toLocaleString()}
              </td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">
                {l.out_tok.toLocaleString()}
              </td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">
                {l.cost_usd.toFixed(4)}
              </td>
              <td
                className={`py-2 pr-2 text-right whitespace-nowrap ${
                  l.status >= 200 && l.status < 300
                    ? "text-[#30d158]"
                    : "text-red-400"
                }`}
              >
                {l.status}
              </td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/25">
                {l.source === "import" ? "歷史" : "即時"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </div>
  );
}
