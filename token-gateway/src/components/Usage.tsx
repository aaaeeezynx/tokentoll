//! Usage 頁：負責版面與鏡頭切換，各鏡頭與圖表已拆到 `usage/`。

import {
  api,
  type Provider,
  type SubscriptionFees,
  type UsageRange,
} from "../lib/api";
import {
  Icon,
} from "./icons";
import {
  PopSelect,
} from "./PopSelect";
import {
  Segmented,
} from "./Segmented";
import {
  RangePicker,
  isHourlyRange,
  stepMsFor,
  type CustomRange,
} from "./RangePicker";
import {
  APP_META,
} from "./logos";
import {
  GridRange,
} from "./usage/usageActivity";
import {
  HistoryImportDialog,
  RecentLogs,
} from "./usage/usageCharts";
import {
  ChannelsLens,
  DailyLens,
  HourlyLens,
  ModelsLens,
  MonthlyLens,
  OverviewLens,
  StatsLens,
} from "./usage/usageLenses";
import {
  AppsLens,
} from "./usage/UsageAppsLens";
import {
  LENSES,
  Lens,
  RANGES,
} from "./usage/usageTypes";
import {
  useQuery,
} from "@tanstack/react-query";
import {
  useState,
} from "react";

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
  const providers = useQuery<Provider[]>({
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
  /** 供「工具」視角點列下鑽用（套用該工具到全域篩選）。 */
  onFilter: (patch: Partial<UsageFilterState>) => void;
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
  const byApp = useQuery({
    queryKey: ["usage_by_app", ...fkey],
    queryFn: () => api.usageByApp(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const hourly = useQuery({
    queryKey: ["usage_hourly", ...fkey],
    queryFn: () => api.usageHourly(range, app || null, pid, model || null, sts, ets),
    refetchInterval: 10000,
  });
  const subFees = useQuery<SubscriptionFees>({
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
      {lens === "apps" && (
        <AppsLens
          byApp={byApp}
          summary={summary}
          onPickApp={(a) => props.onFilter({ app: a })}
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
