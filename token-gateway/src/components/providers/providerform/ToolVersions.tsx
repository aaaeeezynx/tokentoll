//! 由 `components/providers/ProviderForm.tsx` 拆分而來。程式碼語意未改，只搬位置。

import {
  useMemo,
  useState,
} from "react";
import {
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  api,
} from "../../../lib/api";
import {
  Icon,
} from "../../icons";
import {
  Logo,
} from "../../logos";

export function ToolVersions() {
  const qc = useQueryClient();
  const vers = useQuery({
    queryKey: ["tool_versions"],
    queryFn: api.toolVersions,
    staleTime: 60_000,
  });
  // 最新版走 registry，慢查詢獨立跑，不擋版本顯示。
  const latestQ = useQuery({
    queryKey: ["tool_latest"],
    queryFn: api.toolLatest,
    staleTime: 5 * 60_000,
    retry: 1,
  });
  const latestMap = useMemo(
    () => new Map((latestQ.data || []).map((l) => [l.app, l] as const)),
    [latestQ.data],
  );
  const [updating, setUpdating] = useState<string | null>(null);
  const [updMsg, setUpdMsg] = useState<{ app: string; ok: boolean; msg: string } | null>(null);
  const runUpdate = async (app: string) => {
    setUpdating(app);
    setUpdMsg(null);
    try {
      const msg = await api.toolUpdate(app);
      setUpdMsg({ app, ok: true, msg });
      void qc.invalidateQueries({ queryKey: ["tool_versions"] });
      void qc.invalidateQueries({ queryKey: ["tool_latest"] });
      void qc.invalidateQueries({ queryKey: ["tools"] });
    } catch (e) {
      setUpdMsg({ app, ok: false, msg: String(e) });
    } finally {
      setUpdating(null);
    }
  };
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <p className="min-w-0 truncate text-xs text-white/30">
          版本取自本機實際安裝；最新版查 registry；更新走官方通道，單次最長 10 分鐘
        </p>
        <button
          className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
          disabled={vers.isFetching || latestQ.isFetching}
          onClick={() => {
            void vers.refetch();
            void latestQ.refetch();
          }}
        >
          <Icon name="refresh" size={12} />
          {vers.isFetching || latestQ.isFetching ? "刷新中…" : "刷新"}
        </button>
      </div>
      {vers.isPending && <p className="text-sm text-white/30">讀取版本中…</p>}
      {vers.isError && (
        <p className="text-sm text-red-400">讀取失敗：{String(vers.error)}</p>
      )}
      {(vers.data || []).map((v) => {
        // 最新態：用 registry 比對；比不到＝檢查中。
        const l = latestMap.get(v.app);
        const upToDate = l?.up_to_date ?? v.up_to_date ?? null;
        const latest = l?.latest ?? null;
        const showUpdate =
          v.installed && v.update_argv.length > 0 && upToDate !== true;
        return (
          <div
            key={v.app}
            className="flex items-center gap-2 rounded-xl border border-white/[0.06] p-3 text-sm"
          >
            <Logo name={v.app} size={17} className="shrink-0" />
            <span className="min-w-0 flex-1 truncate font-semibold text-white/85">
              {v.display}
            </span>
            {!v.installed ? (
              <span className="shrink-0 text-xs text-white/20">未安裝</span>
            ) : upToDate === true ? (
              <span className="flex shrink-0 items-center gap-1.5">
                <span className="font-mono text-xs text-[#30d158]">
                  {v.version ?? "未知版本"}
                </span>
                <span className="rounded-full bg-[#30d158]/10 px-2 py-0.5 text-[10px] font-medium text-[#30d158]">
                  已是最新
                </span>
              </span>
            ) : (
              <span className="shrink-0 font-mono text-xs text-white/45">
                {v.version ?? "未知版本"}
                {upToDate === false && latest != null && (
                  <span className="text-amber-300/90"> → {latest}</span>
                )}
                {upToDate == null && (
                  <span className="text-white/25">
                    {latestQ.isPending || latestQ.isFetching ? "（檢查中…）" : "（未查到最新版）"}
                  </span>
                )}
              </span>
            )}
            {showUpdate && (
              <button
                className="flex shrink-0 items-center gap-1 rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white disabled:opacity-40"
                title={v.update_label ?? "更新"}
                disabled={updating === v.app}
                onClick={() => void runUpdate(v.app)}
              >
                <Icon name="download" size={12} />
                {updating === v.app ? "更新中…" : "更新"}
              </button>
            )}
          </div>
        );
      })}
      {updMsg && (
        <p
          className={`mt-1.5 text-xs break-all whitespace-pre-wrap ${
            updMsg.ok ? "text-white/45" : "text-red-400"
          }`}
        >
          {updMsg.msg}
        </p>
      )}
    </div>
  );
}
