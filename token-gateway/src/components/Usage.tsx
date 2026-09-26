import { useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { useQuery, useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import {
  api,
  type HistoryScanTool,
  type TrendAppRow,
  type TrendBucket,
  type UsageRange,
  type UsageSummary,
  type ModelStat,
  type ProviderStat,
  type HourlyBucket,
  type SubscriptionFees,
  type SubscriptionQuota,
  type QuotaWindow,
} from "../lib/api";
import { Icon, type IconName } from "./icons";
import { PopSelect } from "./PopSelect";
import { Segmented } from "./Segmented";
import {
  RangePicker,
  isHourlyRange,
  stepMsFor,
  type CustomRange,
} from "./RangePicker";
import { APP_META, Logo, appLabel } from "./logos";

const RANGES: { id: UsageRange; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "7d", label: "最近 7 天" },
  { id: "30d", label: "最近 30 天" },
];

const LENSES: { id: LensId; label: string; icon: IconName }[] = [
  { id: "overview", label: "總覽", icon: "chart-bar-fill" },
  { id: "models", label: "模型", icon: "cpu" },
  { id: "monthly", label: "月", icon: "calendar" },
  { id: "daily", label: "日", icon: "calendar-day" },
  { id: "hourly", label: "時段", icon: "clock" },
  { id: "stats", label: "統計", icon: "chart-line" },
  { id: "channels", label: "來源", icon: "switch" },
];

type LensId = "overview" | "models" | "monthly" | "daily" | "hourly" | "stats" | "channels";

type Lens = (typeof LENSES)[number]["id"];

function fmtInt(n: number): string {
  return Math.round(n).toLocaleString();
}

function fmtCost(n: number): string {
  return n.toFixed(4);
}

function fmtPct(n: number): string {
  return `${(n * 100).toFixed(1)}%`;
}

function fmtBucket(ts: number, hourly: boolean): string {
  const d = new Date(ts);
  if (hourly) {
    return `${d.getHours().toString().padStart(2, "0")}:00`;
  }
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** 各本機工具的主題色（堆疊柱狀圖用）。 */
const APP_COLORS: Record<string, string> = {
  claude: "#d97757", // Anthropic coral
  codex: "#34d399", // OpenAI mint
  opencode: "#f5f5f7", // 白
  hermes: "#a78bfa", // violet
  dsh: "#4d6bfe", // DeepSeek blue
  cursor: "#94a3b8", // steel gray（呼應 Cursor 黑白品牌）
  antigravity: "#fbbf24",
  unknown: "rgba(255,255,255,0.30)",
};
const APP_ORDER = ["claude", "codex", "opencode", "hermes", "dsh", "cursor", "antigravity", "unknown"];
function appColor(app: string): string {
  return APP_COLORS[app] ?? "#8e8e93";
}

interface StackSeg {
  app: string;
  tokens: number;
}
interface StackBucket {
  ts: number;
  total: number;
  requests: number;
  cost_usd: number;
  segs: StackSeg[];
}

function fillAppBuckets(
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
type TipRow = { dot?: string; label: string; value: string };

function HoverTip(props: {
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
function TrendChart(props: {
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

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d
    .getSeconds()
    .toString()
    .padStart(2, "0")}`;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

// ── 歷史回填對話框 ─────────────────────────────────────────────────────────

function HistoryImportDialog(props: { onClose: () => void }) {
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

function timeAgo(ts: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (s < 60) return `${s} 秒前更新`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分鐘前更新`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小時前更新`;
  return `${Math.floor(h / 24)} 天前更新`;
}

function RecentLogs() {
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

type GridRange = "30d" | "90d" | "180d" | "365d";
const GRID_RANGES: {
  id: GridRange;
  label: string;
  days: number;
  cols: number;
  cell?: number;
}[] = [
  { id: "30d", label: "月", days: 30, cols: 10, cell: 34 },
  { id: "90d", label: "季", days: 90, cols: 30 },
  { id: "180d", label: "半年", days: 180, cols: 45 },
  { id: "365d", label: "年", days: 365, cols: 61 },
];

type DaySlot = {
  key: string;
  label: string;
  tokens: number;
  requests: number;
  cost: number;
  future?: boolean;
};

function DayCell(props: {
  d: DaySlot;
  s: number;
  rad: number;
  color: string;
  delay: number;
  onTip?: (d: DaySlot, rect: DOMRect) => void;
  onTipEnd?: () => void;
}) {
  const { d } = props;
  return (
    <div
      onMouseEnter={(e) =>
        props.onTip?.(d, e.currentTarget.getBoundingClientRect())
      }
      onMouseLeave={() => props.onTipEnd?.()}
      className="day-cell shrink-0 anim-rise"
      style={{
        width: props.s,
        height: props.s,
        borderRadius: props.rad,
        backgroundColor: props.color,
        animationDelay: `${props.delay}ms`,
      }}
    />
  );
}

function ContributionGrid(props: {
  data: TrendBucket[];
  range: GridRange;
  onRange: (r: GridRange) => void;
}) {
  const def = GRID_RANGES.find((g) => g.id === props.range)!;
  // 量測可用寬度：格子按檔位縮放，寬度填滿、高度收斂。
  // callback ref：換檔重掛時也會重新綁定 observer。
  const [wrapEl, setWrapEl] = useState<HTMLDivElement | null>(null);
  const [wrapW, setWrapW] = useState(900);
  useLayoutEffect(() => {
    if (!wrapEl) return;
    const ro = new ResizeObserver((es) => {
      const w = es[0]?.contentRect.width;
      if (w && w > 0) setWrapW(w);
    });
    ro.observe(wrapEl);
    return () => ro.disconnect();
  }, [wrapEl]);
  // 窄窗：月視圖收起左右裝飾面板，中央單面板流式填滿（經典三面板需 ~340px+）
  const compact = wrapW < 560;
  const perCol = wrapW / def.cols;
  const gap = perCol > 28 ? 6 : perCol > 16 ? 4 : 3;
  const s =
    !compact && def.cell
      ? def.cell
      : Math.max(6, Math.floor((wrapW - (def.cols - 1) * gap) / def.cols));
  const rad = Math.min(10, Math.max(3, Math.round(s * 0.2)));
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [tip, setTip] = useState<{ d: DaySlot; rect: DOMRect } | null>(null);
  const cellHandlers = {
    onTip: (d: DaySlot, rect: DOMRect) => setTip({ d, rect }),
    onTipEnd: () => setTip(null),
  };

  const { centerRows, prevRows, nextRows, total, maxTokens } = useMemo(() => {
    const byDay = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of props.data) {
      const d = new Date(b.ts);
      const k = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      const e = byDay.get(k) ?? { tokens: 0, cost: 0, requests: 0 };
      e.tokens += b.tokens;
      e.cost += b.cost_usd;
      e.requests += b.requests;
      byDay.set(k, e);
    }
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    const t0 = midnight.getTime();
    type Slot = {
      key: string;
      label: string;
      tokens: number;
      cost: number;
      requests: number;
      future?: boolean;
    };
    // 月檔多建前 30 天（左側上月真實數據）
    const needDays = props.range === "30d" ? 60 : def.days;
    const slots: Slot[] = [];
    for (let i = needDays - 1; i >= 0; i--) {
      const d = new Date(t0 - i * 86400000);
      const k = `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      const v = byDay.get(k);
      slots.push({
        key: k,
        label: `${d.getMonth() + 1}/${d.getDate()}`,
        tokens: v?.tokens ?? 0,
        cost: v?.cost ?? 0,
        requests: v?.requests ?? 0,
      });
    }
    const center =
      props.range === "30d" ? slots.slice(-30) : slots;
    const prev = props.range === "30d" ? slots.slice(0, 30) : [];
    // 下月：未來佔位（無數據，純裝飾）
    const next: Slot[] = [];
    if (props.range === "30d") {
      for (let i = 1; i <= 30; i++) {
        const d = new Date(t0 + i * 86400000);
        next.push({
          key: `f${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`,
          label: `${d.getMonth() + 1}/${d.getDate()}`,
          tokens: 0,
          cost: 0,
          requests: 0,
          future: true,
        });
      }
    }
    let total = 0;
    let maxTokens = 1;
    for (const sl of center) {
      total += sl.tokens;
      if (sl.tokens > maxTokens) maxTokens = sl.tokens;
    }
    const chunk = (arr: Slot[], n: number): Slot[][] => {
      const out: Slot[][] = [];
      for (let r = 0; r < arr.length; r += n) out.push(arr.slice(r, r + n));
      return out;
    };
    return {
      centerRows: chunk(center, def.cols),
      prevRows: chunk(prev, 10),
      nextRows: chunk(next, 10),
      total,
      maxTokens,
    };
  }, [props.data, props.range, def.days, def.cols]);

  const color = (tokens: number) => {
    if (tokens === 0) return "rgba(255,255,255,0.04)";
    const ratio = tokens / maxTokens;
    if (ratio > 0.75) return "color-mix(in srgb, var(--mac-accent) 85%, transparent)";
    if (ratio > 0.5) return "color-mix(in srgb, var(--mac-accent) 62%, transparent)";
    if (ratio > 0.25) return "color-mix(in srgb, var(--mac-accent) 40%, transparent)";
    return "color-mix(in srgb, var(--mac-accent) 20%, transparent)";
  };

  return (
    <div className="glass relative min-w-0 p-4" ref={rootRef}>
      <div className="mb-2.5 flex min-w-0 flex-wrap items-center gap-2">
        <div className="shrink-0 text-sm font-semibold tracking-tight text-white/80">
          用量日曆
        </div>
        <span className="text-[11px] text-white/45">
          共 {fmtInt(total)} tokens
        </span>
        <span className="flex-1" />
      </div>
      <div className="mb-2.5">
        <Segmented
          className="w-full"
          value={props.range}
          onChange={(v) => props.onRange(v as GridRange)}
          btnClassName="py-1 text-[11px] font-medium"
          options={GRID_RANGES.map((g) => ({ id: g.id, label: g.label }))}
        />
      </div>
      {/* 內襯留白：hover 放大/光暈不觸發滾動條 */}
      <div ref={setWrapEl} key={props.range} className="overflow-x-auto px-2 py-2.5">
        {props.range === "30d" && !compact ? (
          <div className="flex min-w-0 items-center gap-3 md:gap-5">
            <div className="flex-1 overflow-hidden brightness-[.5]">
              <div className="flex flex-col items-end" style={{ gap }}>
                {prevRows.map((row, ri) => (
                  <div key={ri} className="flex justify-end" style={{ gap }}>
                    {row.map((d, i) => (
                      <DayCell
                        key={d.key}
                        d={d}
                        s={s}
                        rad={rad}
                        color={color(d.tokens)}
                        delay={Math.min((ri * row.length + i) * 1.5, 300)}
                        {...cellHandlers}
                      />
                    ))}
                  </div>
                ))}
              </div>
            </div>
            <div className="flex shrink-0 flex-col" style={{ gap }}>
              {centerRows.map((row, ri) => (
                <div key={ri} className="flex" style={{ gap }}>
                  {row.map((d, i) => (
                    <DayCell
                      key={d.key}
                      d={d}
                      s={s}
                      rad={rad}
                      color={color(d.tokens)}
                      delay={Math.min((ri * row.length + i) * 1.5, 300)}
                      {...cellHandlers}
                    />
                  ))}
                </div>
              ))}
            </div>
            <div className="flex-1 overflow-hidden brightness-[.5]">
              <div className="flex flex-col items-start" style={{ gap }}>
                {nextRows.map((row, ri) => (
                  <div key={ri} className="flex justify-start" style={{ gap }}>
                    {row.map((d, i) => (
                      <DayCell
                        key={d.key}
                        d={d}
                        s={s}
                        rad={rad}
                        color={color(d.tokens)}
                        delay={Math.min((ri * row.length + i) * 1.5, 300)}
                        {...cellHandlers}
                      />
                    ))}
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="mx-auto flex w-max min-w-full flex-col items-center" style={{ gap }}>
            {centerRows.map((row, ri) => (
              <div key={ri} className="flex" style={{ gap }}>
                {row.map((d, i) => (
                  <DayCell
                    key={d.key}
                    d={d}
                    s={s}
                    rad={rad}
                    color={color(d.tokens)}
                    delay={Math.min((ri * row.length + i) * 1.5, 300)}
                    {...cellHandlers}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="mt-1.5 flex items-center gap-1 text-[10px] text-white/35">
        <span>少</span>
        {[0.2, 0.4, 0.62, 0.85].map((op) => (
          <div
            key={op}
            className="h-2.5 w-2.5 rounded-[3px]"
            style={{ backgroundColor: `color-mix(in srgb, var(--mac-accent) ${op * 100}%, transparent)` }}
          />
        ))}
        <span>多</span>
      </div>
      <HoverTip
        rect={tip?.rect ?? null}
        rootRef={rootRef}
        title={
          tip
            ? `${tip.d.label}${tip.d.future ? " · 未來" : ""}`
            : undefined
        }
        rows={
          tip && !tip.d.future
            ? [
                { label: "Tokens", value: fmtInt(tip.d.tokens) },
                { label: "請求", value: fmtInt(tip.d.requests) },
                { label: "費用", value: `$${fmtCost(tip.d.cost)}` },
              ]
            : []
        }
      >
        {tip && !tip.d.future && tip.d.tokens > 0 && (
          <div className="mt-1.5 flex items-center gap-2">
            <div className="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-white/[0.07]">
              <div
                className="h-full rounded-full"
                style={{
                  width: `${Math.max(3, (tip.d.tokens / maxTokens) * 100)}%`,
                  background: "var(--mac-accent)",
                }}
              />
            </div>
            <span className="shrink-0 text-[10px] text-white/30">
              {((tip.d.tokens / maxTokens) * 100).toFixed(0)}% 峰值
            </span>
          </div>
        )}
      </HoverTip>
    </div>
  );
}

// ── 子視角：總覽 ─────────────────────────────────────────────────────────

// ── 訂閱額度（OpenCode Go / Claude / ChatGPT）：官方額度百分比 + 重置倒計時 ──

function fmtRemain(iso: string | null): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "—";
  let s = Math.floor((t - Date.now()) / 1000);
  if (s <= 0) return "已重置";
  const d = Math.floor(s / 86400);
  s %= 86400;
  const h = Math.floor(s / 3600);
  s %= 3600;
  const m = Math.floor(s / 60);
  if (d > 0) return `${d}天${h}時`;
  if (h > 0) return `${h}時${m}分`;
  if (m > 0) return `${m}分`;
  return `${s}秒`;
}

/** Apple 健康風圓環：進度弧 + 中心百分比，下方窗口名與重置倒計時。顏色跟隨系統強調色。 */
/** 訂閱圓環統一尺寸：所有方案、所有窗口共用同一組數字，視覺保證一致。 */
const RING_BOX = 76; // 外層寬 px
const RING_DIAL = 68; // 錶盤直徑 px
const RING_TEXT = 13; // 中心百分比字號 px
function QuotaRing(props: { w: QuotaWindow }) {
  const { w } = props;
  const pct = Math.max(0, Math.min(100, w.percent));
  const r = 15.5;
  const c = 2 * Math.PI * r;
  const col = "var(--mac-accent)";
  return (
    <div
      className="flex shrink-0 flex-col items-center gap-1.5"
      style={{ width: RING_BOX }}
    >
      <div className="relative" style={{ width: RING_DIAL, height: RING_DIAL }}>
        <svg viewBox="0 0 36 36" className="h-full w-full -rotate-90">
          <circle
            cx="18"
            cy="18"
            r={r}
            fill="none"
            stroke="rgba(255,255,255,0.08)"
            strokeWidth="3.4"
          />
          <circle
            cx="18"
            cy="18"
            r={r}
            fill="none"
            stroke={col}
            strokeWidth="3.4"
            strokeLinecap="round"
            strokeDasharray={`${(pct / 100) * c} ${c}`}
            style={{
              transition:
                "stroke-dasharray 0.6s var(--mac-ease), stroke 0.3s var(--mac-ease)",
              filter:
                "drop-shadow(0 0 4px color-mix(in srgb, var(--mac-accent) 40%, transparent))",
            }}
          />
        </svg>
        <div
          className="absolute inset-0 flex items-center justify-center font-semibold tabular-nums"
          style={{ color: col, fontSize: RING_TEXT }}
        >
          {w.percent < 10 ? w.percent.toFixed(1) : Math.round(w.percent)}%
        </div>
      </div>
      <div className="text-center leading-tight">
        <div className="text-[11px] text-white/55">{w.label}</div>
        <div className="text-[10px] text-white/30">
          {w.resets_at ? `重置 ${fmtRemain(w.resets_at)}` : "—"}
        </div>
      </div>
    </div>
  );
}

function QuotaCard(props: { q: SubscriptionQuota }) {
  const { q } = props;
  const ok = q.status === "ok" && q.windows.length > 0;
  // 名稱去掉括號說明（如「OpenCode Go（opencode-go）」→「OpenCode Go」）
  const title = q.title.replace(/[（(][^（）()]*[）)]/g, "").trim() || q.title;
  return (
    <div className="glass-float float-hover anim-rise w-fit max-w-full min-w-0 p-4">
      <div className="mb-2 flex min-w-0 items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-white/85">
          {title}
        </span>
      </div>
      {ok ? (
        <div className="flex flex-wrap items-start justify-around gap-x-2 gap-y-3">
          {q.windows.map((w) => (
            <QuotaRing key={w.label} w={w} />
          ))}
        </div>
      ) : (
        <p className="text-xs break-words text-white/35">
          {q.message || "暫無額度數據"}
        </p>
      )}
    </div>
  );
}

function QuotaSection() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ["quota_all"],
    queryFn: api.quotaQueryAll,
    staleTime: 5 * 60_000,
    refetchInterval: 5 * 60_000,
  });
  // 只顯示實際有訂閱/可查到額度的方案；未訂閱或無憑證的不佔位
  const list = (q.data || []).filter(
    (x) => x.status === "ok" && x.windows.length > 0,
  );
  if (!q.isPending && list.length === 0) return null;
  return (
    <div className="glass min-w-0 p-4 md:p-5">
      <div className="mb-3 flex items-center gap-2">
        <span className="text-sm font-semibold tracking-tight text-white/80">
          訂閱額度
        </span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-white/25">
          {q.isFetching
            ? "查詢中…"
            : q.dataUpdatedAt
              ? `更新於 ${new Date(q.dataUpdatedAt).toLocaleTimeString()}`
              : ""}
        </span>
        <button
          className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
          disabled={q.isFetching}
          onClick={() => void qc.invalidateQueries({ queryKey: ["quota_all"] })}
        >
          <Icon name="refresh" size={11} />
          重新查詢
        </button>
      </div>
      {q.isPending ? (
        <p className="text-sm text-white/30">查詢中…</p>
      ) : (
        <div className="flex min-w-0 flex-wrap items-start gap-3">
          {list.map((x) => (
            <QuotaCard key={x.key} q={x} />
          ))}
        </div>
      )}
    </div>
  );
}

function OverviewLens(props: {
  summary: UseQueryResult<UsageSummary, unknown>;
  trendByApp: UseQueryResult<TrendAppRow[], unknown>;
  chartKey: string;
  stepMs: number;
  hourly: boolean;
  gridTrend: UseQueryResult<TrendBucket[], unknown>;
  gridRange: GridRange;
  onGridRange: (r: GridRange) => void;
  subFees: UseQueryResult<SubscriptionFees, unknown>;
}) {
  const { summary, trendByApp, chartKey, stepMs, hourly, gridTrend, gridRange, onGridRange, subFees } = props;
  const subTotal = subFees.data?.total_fee ?? 0;
  const grandTotal = (summary.data?.cost_usd ?? 0) + subTotal;
  return (
    <div className="min-w-0 space-y-4">
      {summary.data && (
        <div className="grid min-w-0 grid-cols-2 gap-3 lg:grid-cols-4">
          {[
            ["總請求", fmtInt(summary.data.requests)],
            ["真實消耗 Tokens", fmtInt(summary.data.total_tokens)],
            ["快取命中率", fmtPct(summary.data.cache_hit_rate)],
            ["請求費用 $", fmtCost(summary.data.cost_usd)],
            ["成功率", fmtPct(summary.data.success_rate)],
            ["本月訂閱費 $", fmtCost(subTotal)],
            ["合計成本 $", fmtCost(grandTotal)],
          ].map(([label, value], i) => (
            <div
              key={label}
              className="glass-float float-hover anim-rise min-w-0 p-4"
              style={{ animationDelay: `${i * 50}ms` }}
            >
              <div className="text-[11px] font-medium text-white/35 uppercase tracking-wider">{label}</div>
              <div className="mt-1.5 truncate text-xl font-semibold text-white/90">{value}</div>
            </div>
          ))}
        </div>
      )}
      {summary.data && subTotal > 0 && (
        <p className="-mt-2 text-[11px] text-white/25">
          合計＝請求費用（當前篩選）＋本月訂閱費（自然月）；超額部分已按基礎價計入請求費用
        </p>
      )}

      <QuotaSection />

      <ContributionGrid
        data={gridTrend.data || []}
        range={gridRange}
        onRange={onGridRange}
      />

      <div className="glass min-w-0 p-3 md:p-5">
        <div className="mb-2 flex min-w-0 flex-wrap items-baseline gap-x-2 text-sm font-semibold tracking-tight text-white/80">
          <span className="shrink-0">Token 趨勢</span>
          {summary.data && (
            <span className="min-w-0 text-xs font-normal break-words text-white/30">
              輸入 {fmtInt(summary.data.in_tok)} · 輸出{" "}
              {fmtInt(summary.data.out_tok)} · 快取讀{" "}
              {fmtInt(summary.data.cache_read)} · 快取建{" "}
              {fmtInt(summary.data.cache_write)}
            </span>
          )}
        </div>
        {trendByApp.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : (
          <TrendChart
            key={chartKey}
            rows={trendByApp.data || []}
            stepMs={stepMs}
            fillToday={chartKey === "today"}
            hourly={hourly}
          />
        )}
      </div>
    </div>
  );
}

// ── 子視角：模型排行 ──────────────────────────────────────────────────────

function ModelsLens(props: {
  byModel: UseQueryResult<ModelStat[], unknown>;
}) {
  const data = props.byModel.data || [];
  const maxCost = Math.max(...data.map((m) => m.cost_usd), 0.001);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">模型排行（按費用）</div>
      <div className="space-y-1.5">
        {data.map((m) => (
          <div key={m.model} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2.5 transition-colors hover:bg-white/[0.055]">
            <div className="min-w-0 flex-1">
              <div className="truncate font-mono text-xs text-white/70">{m.model}</div>
              <div className="text-[10px] text-white/25">
                {fmtInt(m.requests)} 請求 · 輸入 {fmtInt(m.in_tok)} · 輸出 {fmtInt(m.out_tok)}
              </div>
            </div>
            <div className="text-right">
              <div className="text-xs font-medium text-white/60">${fmtCost(m.cost_usd)}</div>
              <div className="mt-0.5 h-1 w-16 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(m.cost_usd / maxCost) * 100}%` }}
                />
              </div>
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
      </div>
    </div>
  );
}

// ── 子視角：月聚合 ────────────────────────────────────────────────────────

function MonthlyLens(props: {
  trend: UseQueryResult<TrendBucket[], unknown>;
  range: UsageRange;
}) {
  const data = useMemo(() => {
    const buckets = props.trend.data || [];
    const byMonth = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of buckets) {
      const d = new Date(b.ts);
      const key = `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, "0")}`;
      const existing = byMonth.get(key) ?? { tokens: 0, cost: 0, requests: 0 };
      existing.tokens += b.tokens;
      existing.cost += b.cost_usd;
      existing.requests += b.requests;
      byMonth.set(key, existing);
    }
    return Array.from(byMonth.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([month, v]) => ({ month, ...v }));
  }, [props.trend.data]);

  const maxTokens = Math.max(...data.map((d) => d.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">月度用量</div>
      <div className="space-y-1.5">
        {data.map((d) => (
          <div key={d.month} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2.5 transition-colors hover:bg-white/[0.055]">
            <div className="w-16 text-xs font-medium text-white/50">{d.month}</div>
            <div className="flex-1">
              <div className="h-4 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-white/35">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
      </div>
    </div>
  );
}

// ── 子視角：日聚合 ────────────────────────────────────────────────────────

function DailyLens(props: {
  trend: UseQueryResult<TrendBucket[], unknown>;
}) {
  const data = useMemo(() => {
    const buckets = props.trend.data || [];
    const byDay = new Map<string, { tokens: number; cost: number; requests: number }>();
    for (const b of buckets) {
      const d = new Date(b.ts);
      const key = `${d.getMonth() + 1}/${d.getDate()}`;
      const existing = byDay.get(key) ?? { tokens: 0, cost: 0, requests: 0 };
      existing.tokens += b.tokens;
      existing.cost += b.cost_usd;
      existing.requests += b.requests;
      byDay.set(key, existing);
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => {
        const [am, ad] = a.split("/").map(Number);
        const [bm, bd] = b.split("/").map(Number);
        return am !== bm ? am - bm : ad - bd;
      })
      .map(([day, v]) => ({ day, ...v }));
  }, [props.trend.data]);

  const maxTokens = Math.max(...data.map((d) => d.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">每日用量</div>
      <div className="space-y-1">
        {data.map((d) => (
          <div key={d.day} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3.5 py-2">
            <div className="w-12 text-[10px] font-medium text-white/40">{d.day}</div>
            <div className="flex-1">
              <div className="h-3 overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                  style={{ width: `${(d.tokens / maxTokens) * 100}%` }}
                />
              </div>
            </div>
            <div className="text-right text-[10px] text-white/30">
              {fmtInt(d.tokens)} tok · ${fmtCost(d.cost)}
            </div>
          </div>
        ))}
        {data.length === 0 && <p className="text-xs text-white/20">暫無數據</p>}
      </div>
    </div>
  );
}

// ── 子視角：時段熱力圖 ──────────────────────────────────────────────────

function HourlyLens(props: {
  hourly: UseQueryResult<HourlyBucket[], unknown>;
}) {
  const data = props.hourly.data || [];
  // 填補 0-23 所有小時
  const filled = Array.from({ length: 24 }, (_, i) => {
    const found = data.find((d) => d.hour === i);
    return found ?? { hour: i, requests: 0, tokens: 0, cost_usd: 0 };
  });
  const maxTokens = Math.max(...filled.map((d) => d.tokens), 1);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [hover, setHover] = useState<{ i: number; rect: DOMRect } | null>(null);
  const hd = hover != null ? filled[hover.i] : null;
  return (
    <div className="glass relative p-5" ref={rootRef}>
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">時段分佈（哪個時段最燒）</div>
      <div className="chart-row flex h-32 items-end gap-0.5">
        {filled.map((d, i) => (
          <div
            key={d.hour}
            onMouseEnter={(e) =>
              setHover({ i, rect: e.currentTarget.getBoundingClientRect() })
            }
            onMouseLeave={() => setHover(null)}
            className="chart-bar bar-grow flex-1 rounded-t"
            style={{
              height: `${Math.max(2, (d.tokens / maxTokens) * 100)}%`,
              animationDelay: `${d.hour * 15}ms`,
              backgroundColor:
                d.tokens === 0
                  ? "rgba(255,255,255,0.03)"
                  : d.tokens / maxTokens > 0.7
                    ? "color-mix(in srgb, var(--mac-accent) 58%, transparent)"
                    : d.tokens / maxTokens > 0.3
                      ? "color-mix(in srgb, var(--mac-accent) 38%, transparent)"
                      : "color-mix(in srgb, var(--mac-accent) 20%, transparent)",
            }}
          />
        ))}
      </div>
      <div className="mt-1 flex gap-0.5 text-[10px] text-white/25">
        {filled.map((d) => (
          <div key={d.hour} className="flex-1 text-center">
            {d.hour % 3 === 0 ? `${d.hour}` : ""}
          </div>
        ))}
      </div>
      <HoverTip
        rect={hover?.rect ?? null}
        rootRef={rootRef}
        title={hd ? `${hd.hour}:00 - ${hd.hour + 1}:00` : undefined}
        rows={
          hd
            ? [
                { label: "Tokens", value: fmtInt(hd.tokens) },
                { label: "請求", value: fmtInt(hd.requests) },
                { label: "費用", value: `$${fmtCost(hd.cost_usd)}` },
              ]
            : []
        }
      />
    </div>
  );
}

// ── 子視角：統計 ─────────────────────────────────────────────────────────

function StatsLens(props: {
  summary: UseQueryResult<UsageSummary, unknown>;
  byModel: UseQueryResult<ModelStat[], unknown>;
  byProvider: UseQueryResult<ProviderStat[], unknown>;
}) {
  const s = props.summary.data;
  const models = props.byModel.data || [];
  const providers = props.byProvider.data || [];
  const topModel = models[0];
  const topProvider = providers[0];
  return (
    <div className="space-y-4">
      <div className="glass p-5">
        <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">摘要</div>
        {s ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
            {[
              ["總請求", fmtInt(s.requests)],
              ["成功請求", fmtInt(s.ok_requests)],
              ["失敗請求", fmtInt(s.requests - s.ok_requests)],
              ["總 Tokens", fmtInt(s.total_tokens)],
              ["輸入 Tokens", fmtInt(s.in_tok)],
              ["輸出 Tokens", fmtInt(s.out_tok)],
              ["快取讀", fmtInt(s.cache_read)],
              ["快取建", fmtInt(s.cache_write)],
              ["總費用 $", fmtCost(s.cost_usd)],
            ].map(([label, value]) => (
              <div key={label}>
                <div className="text-[10px] text-white/25">{label}</div>
                <div className="text-sm font-medium text-white/70">{value}</div>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-white/30">載入中…</p>
        )}
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-white/35">最常使用模型</div>
          <div className="mt-1.5 text-sm font-semibold text-white/80">
            {topModel ? topModel.model : "—"}
          </div>
          <div className="text-[10px] text-white/25">
            {topModel ? `${fmtInt(topModel.tokens)} tokens · $${fmtCost(topModel.cost_usd)}` : ""}
          </div>
        </div>
        <div className="glass p-5">
          <div className="text-[10px] font-medium text-white/35">最常用來源</div>
          <div className="mt-1.5 text-sm font-semibold text-white/80">
            {topProvider ? topProvider.provider_name : "—"}
          </div>
          <div className="text-[10px] text-white/25">
            {topProvider ? `${fmtInt(topProvider.tokens)} tokens · $${fmtCost(topProvider.cost_usd)}` : ""}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── 子視角：來源 ─────────────────────────────────────────────────────────

function ChannelsLens(props: {
  byProvider: UseQueryResult<ProviderStat[], unknown>;
}) {
  const data = props.byProvider.data || [];
  const maxTokens = Math.max(...data.map((p) => p.tokens), 1);
  return (
    <div className="glass p-5">
      <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">來源統計</div>
      <div className="overflow-x-auto">
      <table className="w-full min-w-[480px] text-left text-xs">
        <thead>
          <tr className="text-white/55">
            <th className="py-1.5 pr-2">來源</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">請求</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">Tokens</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">費用$</th>
            <th className="py-1.5 pr-2 text-right whitespace-nowrap">佔比</th>
          </tr>
        </thead>
        <tbody>
          {data.map((p) => (
            <tr key={p.provider_id ?? -1} className="border-t border-white/[0.04]">
              <td className="py-2 pr-2 text-white/70">{p.provider_name}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{p.requests}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{fmtInt(p.tokens)}</td>
              <td className="py-2 pr-2 text-right whitespace-nowrap text-white/60">{fmtCost(p.cost_usd)}</td>
              <td className="py-2 pr-2 text-right">
                <div className="flex items-center justify-end gap-1.5">
                  <div className="h-1.5 w-12 overflow-hidden rounded-full bg-white/[0.05]">
                    <div
                      className="h-full rounded-full bg-(--mac-accent)/45 bar-anim"
                      style={{ width: `${(p.tokens / maxTokens) * 100}%` }}
                    />
                  </div>
                  <span className="text-[10px] text-white/30">{fmtPct(p.tokens / (data.reduce((s, x) => s + x.tokens, 0) || 1))}</span>
                </div>
              </td>
            </tr>
          ))}
          {data.length === 0 && (
            <tr><td className="py-2 text-white/20" colSpan={5}>暫無數據</td></tr>
          )}
        </tbody>
      </table>
      </div>
    </div>
  );
}

// ── 篩選列（常駐頂部 toolbar，狀態由 Shell 持有） ─────────────────────────

export interface UsageFilterState {
  range: UsageRange;
  app: string;
  providerId: string;
  model: string;
  custom: CustomRange | null;
}

export function UsageFilterBar(props: {
  filter: UsageFilterState;
  onFilter: (patch: Partial<UsageFilterState>) => void;
  onImport: () => void;
}) {
  const { filter, onFilter } = props;
  const { range, app, providerId, model } = filter;
  const providers = useQuery({
    queryKey: ["providers"],
    queryFn: api.providersList,
  });
  const selCls = "field px-3 py-1.5 text-[13px] text-white/80";
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2 basis-full sm:basis-auto">
      <RangePicker
        presets={RANGES.map((r) => ({ id: r.id, label: r.label }))}
        range={range}
        custom={filter.custom}
        onPreset={(v) => onFilter({ range: v as UsageRange })}
        onCustom={(c) => onFilter({ custom: c })}
        btnClassName="px-3 py-1 text-[13px] font-medium"
      />
      </div>
      <PopSelect
        className="min-w-[140px] flex-1"
        value={app}
        onChange={(v) => onFilter({ app: v })}
        options={[
          { value: "", label: "全部本機工具" },
          ...APP_META.map((m) => ({ value: m.id, label: m.label, logo: m.id })),
        ]}
      />
      <PopSelect
        className="min-w-[140px] flex-1"
        value={providerId}
        onChange={(v) => onFilter({ providerId: v })}
        options={[
          { value: "", label: "全部來源" },
          ...(providers.data || []).map((p) => ({
            value: String(p.id),
            label: p.name,
            hint: p.base_url,
          })),
        ]}
      />
      <span className="relative min-w-[140px] flex-1">
        <Icon
          name="search"
          size={13}
          className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-white/25"
        />
        <input
          className={`${selCls} w-full pl-8`}
          placeholder="模型關鍵字"
          value={model}
          onChange={(e) => onFilter({ model: e.target.value })}
        />
      </span>
      <button
        className="btn-ghost flex shrink-0 items-center gap-1.5 px-3 py-1.5 text-[13px]"
        onClick={props.onImport}
        title="離線解析本機會話寫入統計"
      >
        <Icon name="download" size={14} />
        歷史回填
      </button>
    </div>
  );
}

// ── 主頁面 ────────────────────────────────────────────────────────────────

export default function UsagePage(props: {
  filter: UsageFilterState;
  showImport: boolean;
  onCloseImport: () => void;
}) {
  const { range, app, providerId, model, custom } = props.filter;
  const [lens, setLens] = useState<Lens>("overview");
  const [gridRange, setGridRange] = useState<GridRange>("30d");
  const sts = custom?.start ?? null;
  const ets = custom?.end ?? null;

  const fkey = [range, sts, ets, app || null, providerId || null, model || null] as const;
  const pid = providerId === "" ? null : Number(providerId);
  const summary = useQuery({
    queryKey: ["usage_summary", ...fkey],
    queryFn: () => api.usageSummary(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const trend = useQuery({
    queryKey: ["usage_trend", ...fkey],
    queryFn: () => api.usageTrend(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const trendByApp = useQuery({
    queryKey: ["usage_trend_by_app", ...fkey],
    queryFn: () => api.usageTrendByApp(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  // 月檔需要前 30 天做左側上月餘暉，查 90 天切片
  const gridQueryRange = gridRange === "30d" ? "90d" : gridRange;
  const gridTrend = useQuery({
    queryKey: ["usage_trend", gridQueryRange, app || null, pid, model || null],
    queryFn: () =>
      api.usageTrend(gridQueryRange, app || null, pid, model || null),
    refetchInterval: 10000,
  });
  const byProvider = useQuery({
    queryKey: ["usage_by_provider", ...fkey],
    queryFn: () => api.usageByProvider(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const byModel = useQuery({
    queryKey: ["usage_by_model", ...fkey],
    queryFn: () => api.usageByModel(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const hourly = useQuery({
    queryKey: ["usage_hourly", ...fkey],
    queryFn: () => api.usageHourly(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const subFees = useQuery({
    queryKey: ["subscription_fees"],
    queryFn: api.subscriptionFees,
    refetchInterval: 60000,
  });
  const stepMs = stepMsFor(range, custom);
  const hourlyMode = isHourlyRange(range, custom);
  const chartKey = custom ? `c${custom.start}-${custom.end}` : range;

  return (
    <div className="space-y-4">
      {props.showImport && (
        <HistoryImportDialog onClose={props.onCloseImport} />
      )}

      {/* ── 透鏡 Segmented（撐滿容器，隨視窗伸縮；過窄時內部橫滑） ── */}
      <div className="overflow-x-auto">
        <Segmented
          value={lens}
          onChange={(v) => setLens(v as Lens)}
          className="w-full min-w-max"
          btnClassName="px-3 py-1.5 text-xs font-medium"
          options={LENSES.map((l) => ({
            id: l.id,
            label: (
              <>
                <Icon name={l.icon} size={13} />
                <span>{l.label}</span>
              </>
            ),
          }))}
        />
      </div>

      {/* ── 子視角渲染 ── */}
      {lens === "overview" && (
        <OverviewLens
          summary={summary}
          trendByApp={trendByApp}
          chartKey={chartKey}
          stepMs={stepMs}
          hourly={hourlyMode}
          gridTrend={gridTrend}
          gridRange={gridRange}
          onGridRange={setGridRange}
          subFees={subFees}
        />
      )}
      {lens === "models" && (
        <ModelsLens byModel={byModel} />
      )}
      {lens === "monthly" && (
        <MonthlyLens trend={trend} range={range} />
      )}
      {lens === "daily" && (
        <DailyLens trend={trend} />
      )}
      {lens === "hourly" && (
        <HourlyLens hourly={hourly} />
      )}
      {lens === "stats" && (
        <StatsLens summary={summary} byModel={byModel} byProvider={byProvider} />
      )}
      {lens === "channels" && (
        <ChannelsLens byProvider={byProvider} />
      )}

      {/* ── 最近請求（常駐底部，表頭含新鮮度＋手動刷新） ── */}
      <div className="glass p-5">
        <RecentLogs />
      </div>
    </div>
  );
}
