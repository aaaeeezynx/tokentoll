//! 由 `Usage.tsx` 拆分而來（原檔 1,685 行）。程式碼語意未改，只搬位置。

import {
  api,
  type HistoryScanTool,
  type TrendAppRow,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  Logo,
  appLabel,
} from "../logos";
import {
  APP_ORDER,
  appColor,
  fmtBucket,
  fmtCost,
  fmtInt,
} from "./usageTypes";
import {
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";

export interface StackSeg {
  app: string;
  tokens: number;
}
export interface StackBucket {
  ts: number;
  total: number;
  requests: number;
  cost_usd: number;
  segs: StackSeg[];
}

export function fillAppBuckets(
  rows: TrendAppRow[],
  step: number,
  fillToday: boolean,
): { buckets: StackBucket[]; apps: string[]; totals: Record<string, number> } {
  if (rows.length === 0 && !fillToday)
    return { buckets: [], apps: [], totals: {} };
  // 今日固定補滿 00:00–23:00 共 24 桶（與後端整點桶對齊，UTC+8 整點一致）
  let minTs: number;
  let maxTs: number;
  if (fillToday) {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    minTs = d.getTime();
    maxTs = minTs + 23 * 3600 * 1000;
  } else {
    if (rows.length === 0) return { buckets: [], apps: [], totals: {} };
    minTs = Math.min(...rows.map((r) => r.ts));
    maxTs = Math.max(...rows.map((r) => r.ts));
  }
  const apps = Array.from(new Set(rows.map((r) => r.app))).sort((a, b) => {
    const oa = APP_ORDER.includes(a) ? APP_ORDER.indexOf(a) : 99;
    const ob = APP_ORDER.includes(b) ? APP_ORDER.indexOf(b) : 99;
    return oa - ob || (a < b ? -1 : 1);
  });
  const byKey = new Map(rows.map((r) => [`${r.ts}|${r.app}`, r]));
  const totals: Record<string, number> = {};
  for (const a of apps) totals[a] = 0;
  const buckets: StackBucket[] = [];
  for (let t = minTs; t <= maxTs; t += step) {
    const segs: StackSeg[] = [];
    let total = 0;
    let requests = 0;
    let cost = 0;
    for (const a of apps) {
      const r = byKey.get(`${t}|${a}`);
      const tokens = r?.tokens ?? 0;
      if (tokens > 0) {
        segs.push({ app: a, tokens });
        totals[a] += tokens;
      }
      total += tokens;
      requests += r?.requests ?? 0;
      cost += r?.cost_usd ?? 0;
    }
    buckets.push({ ts: t, total, requests, cost_usd: cost, segs });
  }
  return { buckets, apps, totals };
}

/** 圖表浮層提示：取代瀏覽器原生 title，定位於錨點元素上方（不足時翻到下方）。 */
export type TipRow = { dot?: string; label: string; value: string };

export function HoverTip(props: {
  rect: DOMRect | null;
  rootRef: RefObject<HTMLDivElement | null>;
  title?: string;
  rows?: TipRow[];
  children?: ReactNode;
}) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    setSize((s) => (s.w === w && s.h === h ? s : { w, h }));
  }, [props.rect, props.title, props.rows, props.children]);
  if (!props.rect) return null;
  const root = props.rootRef.current?.getBoundingClientRect();
  if (!root) return null;
  const w = size.w || 180;
  const h = size.h || 72;
  let left = props.rect.left - root.left + props.rect.width / 2 - w / 2;
  left = Math.max(4, Math.min(left, Math.max(4, root.width - w - 4)));
  let top = props.rect.top - root.top - h - 10;
  if (top < 4) top = props.rect.bottom - root.top + 10;
  return (
    <div
      ref={boxRef}
      className="tip-pop glass-float pointer-events-none absolute z-30 px-3 py-2"
      style={{
        left,
        top,
        minWidth: 170,
        maxWidth: 280,
        visibility: size.w ? "visible" : "hidden",
      }}
    >
      {props.title && (
        <div className="mb-1 text-[11px] font-semibold text-white/85">
          {props.title}
        </div>
      )}
      {props.rows && props.rows.length > 0 && (
        <div className="space-y-0.5">
          {props.rows.map((r) => (
            <div
              key={r.label}
              className="flex items-center gap-2 text-[11px] leading-tight"
            >
              {r.dot ? (
                <span
                  className="h-2 w-2 shrink-0 rounded-sm"
                  style={{ background: r.dot, boxShadow: `0 0 5px 0 ${r.dot}` }}
                />
              ) : (
                <span className="w-2 shrink-0" />
              )}
              <span className="min-w-0 flex-1 truncate text-white/40">
                {r.label}
              </span>
              <span className="shrink-0 font-medium tabular-nums text-white/80">
                {r.value}
              </span>
            </div>
          ))}
        </div>
      )}
      {props.children}
    </div>
  );
}

/** 最高柱 84% 高、單柱最寬 36px、按本機工具主題色堆疊。 */
export function TrendChart(props: {
  rows: TrendAppRow[];
  stepMs: number;
  fillToday: boolean;
  hourly: boolean;
}) {
  const { buckets, apps, totals } = useMemo(
    () => fillAppBuckets(props.rows, props.stepMs, props.fillToday),
    [props.rows, props.stepMs, props.fillToday],
  );
  if (buckets.length === 0) {
    return <p className="text-sm text-white/20">該範圍內暫無數據。</p>;
  }
  const max = Math.max(...buckets.map((b) => b.total), 1);
  const labelEvery = Math.max(1, Math.floor(buckets.length / 8));
  const spring = "duration-500 ease-[cubic-bezier(0.32,0.72,0,1)]";
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [hover, setHover] = useState<{ i: number; rect: DOMRect } | null>(null);
  const hb = hover != null ? buckets[hover.i] : null;
  return (
    <div className="anim-rise relative" ref={rootRef}>
      <div className="overflow-x-auto">
      <div className="min-w-0">
      <div className="chart-row flex h-36 items-end justify-center gap-1">
        {buckets.map((b, i) => {
          const h = b.total === 0 ? 3 : Math.max(5, (b.total / max) * 84);
          return (
            <div
              key={b.ts}
              onMouseEnter={(e) =>
                setHover({ i, rect: e.currentTarget.getBoundingClientRect() })
              }
              onMouseLeave={() => setHover(null)}
              className="chart-bar bar-grow flex w-full min-w-[4px] max-w-9 flex-1 flex-col justify-end overflow-hidden rounded-t border border-b-0 border-white/10 bg-white/[0.05] shadow-[inset_0_1px_0_rgba(255,255,255,0.18)]"
              style={{
                height: `${h}%`,
                animationDelay: `${Math.min(i * 12, 400)}ms`,
              }}
            >
              {b.segs.map((s) => {
                const c = appColor(s.app);
                return (
                  <div
                    key={s.app}
                    className={`w-full shrink-0 transition-[height] ${spring}`}
                    style={{
                      height: `${(s.tokens / b.total) * 100}%`,
                      background: `linear-gradient(180deg, color-mix(in srgb, ${c} 55%, white), ${c})`,
                      borderTop: "1px solid rgba(255,255,255,0.14)",
                      opacity: 0.9,
                    }}
                  />
                );
              })}
            </div>
          );
        })}
      </div>
      <HoverTip
        rect={hover?.rect ?? null}
        rootRef={rootRef}
        title={hb ? `${fmtBucket(hb.ts, props.hourly)} · ${fmtInt(hb.total)} tokens` : undefined}
        rows={
          hb
            ? [
                { label: "請求", value: fmtInt(hb.requests) },
                { label: "費用", value: `$${fmtCost(hb.cost_usd)}` },
                ...hb.segs.map((s) => ({
                  dot: appColor(s.app),
                  label: s.app,
                  value: `${fmtInt(s.tokens)} · ${hb.total ? ((s.tokens / hb.total) * 100).toFixed(0) : 0}%`,
                })),
              ]
            : []
        }
      />
      {apps.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1">
          {apps.map((a) => (
            <span
              key={a}
              className="flex items-center gap-1.5 text-[10px] text-white/40"
            >
              <span
                className="inline-block h-2 w-2 rounded-sm"
                style={{
                  background: appColor(a),
                  boxShadow: `0 0 6px 0 ${appColor(a)}`,
                }}
              />
              {a} · {fmtInt(totals[a])}
            </span>
          ))}
        </div>
      )}
      <div className="mt-1 flex justify-center gap-1 text-[10px] text-white/25">
        {buckets.map((b, i) => (
          <div
            key={b.ts}
            className="w-full min-w-[4px] max-w-9 flex-1 truncate text-center"
          >
            {i % labelEvery === 0 ? fmtBucket(b.ts, props.hourly) : ""}
          </div>
        ))}
      </div>
      </div>
      </div>
    </div>
  );
}

export function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d
    .getSeconds()
    .toString()
    .padStart(2, "0")}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

// ── 歷史回填對話框 ─────────────────────────────────────────────────────────

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
            `${p.tool}: ${p.rows} 筆 / ${fmtInt(p.tokens)} tokens / $${fmtCost(p.cost_usd)}`,
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

// ── 用量日曆（GitHub 式週列） ────────────────────────────────────────────
