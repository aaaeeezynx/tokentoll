/**
 * 診斷頁的「來源健康狀態」區塊：斷路器現在跳開了哪些來源。
 *
 * 規則（見 `src-tauri/src/proxy/health.rs`）：來源**連續失敗 3 次**就冷卻 60 秒，
 * 冷卻期間只是被排到候選佇列的最後（不是跳過 —— 全部來源都掛時還是要有人上）。
 * 只有「連不上」與「上游 5xx」算失敗：4xx 代表來源活著，不算。
 */
import type { SourceHealth } from "../../lib/apiTypes";

/** 「多久以前」的人話版本。 */
function ago(secs: number | null): string {
  if (secs === null) return "—";
  if (secs < 60) return `${secs} 秒前`;
  if (secs < 3600) return `${Math.floor(secs / 60)} 分鐘前`;
  return `${Math.floor(secs / 3600)} 小時前`;
}

export function SourceHealthList({ list }: { list: SourceHealth[] }) {
  if (list.length === 0) {
    return (
      <p className="text-[13px] text-fg/35">
        還沒有任何來源健康紀錄 —— 沒有請求過的來源不會有紀錄（沒有證據就不假裝健康）。
      </p>
    );
  }
  return (
    <div className="space-y-1.5">
      {list.map((h) => (
        <div
          key={h.provider_id}
          className="flex flex-wrap items-center gap-2 rounded-md bg-fg/[0.04] px-3 py-2"
        >
          <span className="min-w-0 truncate text-[13px] text-fg/80">
            {h.provider_name || (
              <span className="text-fg/35">已刪除的渠道 #{h.provider_id}</span>
            )}
          </span>
          <span className="font-mono text-[10px] text-fg/25">
            #{h.provider_id}
          </span>
          <span className="rounded bg-fg/[0.06] px-1.5 py-px font-mono text-[11px] text-fg/45">
            {h.api_format || "—"}
          </span>
          <span
            className="rounded px-1.5 py-px text-[11px]"
            style={
              h.open
                ? { background: "rgba(255,69,58,0.16)", color: "#ff6961" }
                : { background: "rgba(48,209,88,0.14)", color: "#30d158" }
            }
          >
            {h.open ? `冷卻中 ${h.cooldown_secs}s` : "正常"}
          </span>
          <span className="flex flex-1 flex-wrap items-center gap-2 text-[11px] text-fg/35">
            {h.consecutive_failures > 0 && (
              <span>
                連續失敗 <span className="text-fg/60">{h.consecutive_failures}</span> 次
              </span>
            )}
            <span>上次失敗 {ago(h.secs_since_failure)}</span>
            <span>上次成功 {ago(h.secs_since_success)}</span>
          </span>
          {h.last_error && (
            <span
              className="w-full min-w-0 truncate font-mono text-[11px] text-fg/30"
              title={h.last_error}
            >
              {h.last_error}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
