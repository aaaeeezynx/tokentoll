/**
 * 故障轉移佇列（P1.4，對標 CC Switch 的 failover queue 畫面）。
 *
 * 這個面板**不自己排序**：它呼叫後端的 `failover_queue`，而那個命令用的就是
 * `proxy_handler` 換手時的同一個查詢（`resolve_model_providers`）＋同一條健康排序。
 * 所以畫面上的順序就是實際會走的順序 —— 一旦有第二份排序邏輯，畫面就會開始說謊。
 *
 * 預覽以「模型」為中心：某個模型被哪些來源登記、順序如何、各自學到什麼協議。
 * 調整順序的方式是**拖曳來源清單**（順序＝來源清單順序），這裡只負責顯示。
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { Icon } from "../icons";
import { PopSelect } from "../PopSelect";

export function FailoverQueue() {
  const [model, setModel] = useState<string>("");
  const models = useQuery({
    queryKey: ["failover_models"],
    queryFn: api.failoverModels,
    refetchInterval: 15000,
  });
  const rows = useQuery({
    queryKey: ["failover_queue", model],
    queryFn: () => api.failoverQueue(model),
    enabled: model.length > 0,
    refetchInterval: 10000,
  });

  const list = models.data ?? [];
  const cur = list.find((m) => m.model === model) ?? null;

  return (
    <div className="glass p-5">
      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
          <Icon name="switch" size={17} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-[15px] font-semibold tracking-tight text-white">
            故障轉移佇列
          </div>
          <div className="text-[11px] text-white/30">
            某個模型打不通時，網關會照這個順序往下換來源（順序＝來源清單順序，可拖曳調整）
          </div>
        </div>
        <div className="w-full sm:w-64">
          <PopSelect
            value={model}
            onChange={setModel}
            options={[
              { value: "", label: "選一個模型…" },
              ...list.map((m) => ({
                value: m.model,
                label: `${m.model}（${m.sources} 個來源）`,
              })),
            ]}
          />
        </div>
      </div>

      {models.isPending ? (
        <p className="text-sm text-white/30">載入中…</p>
      ) : list.length === 0 ? (
        <p className="text-[13px] text-white/35">
          目前沒有可排隊的模型 —— 先在來源裡登記模型（來源 → 模型 → 取得模型清單）。
        </p>
      ) : !model ? (
        <p className="text-[13px] text-white/35">
          選一個模型，看它在哪些來源上架、換手順序如何。
        </p>
      ) : rows.isPending ? (
        <p className="text-sm text-white/30">載入中…</p>
      ) : rows.isError ? (
        <p className="text-sm text-white/50">讀取失敗：{String(rows.error)}</p>
      ) : (rows.data?.length ?? 0) === 0 ? (
        <p className="text-[13px] text-white/35">
          這個模型目前沒有可用的來源（來源或模型被停用了）。
        </p>
      ) : (
        <>
          <div className="space-y-1.5">
            {rows.data?.map((r, i) => (
              <div
                key={`${r.provider_id}-${r.api_format}`}
                className="flex flex-wrap items-center gap-2 rounded-md bg-white/[0.04] px-3 py-2"
              >
                <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-white/[0.08] text-[10px] font-semibold text-white/60">
                  {i + 1}
                </span>
                <span className="min-w-0 truncate text-[13px] text-white/85">
                  {r.provider_name || (
                    <span className="text-white/35">已刪除的渠道 #{r.provider_id}</span>
                  )}
                </span>
                <span className="rounded bg-white/[0.06] px-1.5 py-px font-mono text-[10px] text-white/40">
                  {r.api_format}
                </span>
                {r.learned_format && (
                  <span
                    className="rounded px-1.5 py-px font-mono text-[10px]"
                    style={{ background: "rgba(48,209,88,0.14)", color: "#30d158" }}
                    title="網關學到：這個模型用這個協議會通"
                  >
                    學到 {r.learned_format}
                  </span>
                )}
                {r.open && (
                  <span
                    className="rounded px-1.5 py-px text-[10px]"
                    style={{ background: "rgba(255,69,58,0.16)", color: "#ff6961" }}
                  >
                    冷卻中 {r.cooldown_secs}s（已排到最後）
                  </span>
                )}
                {!r.enabled && (
                  <span className="rounded bg-white/[0.06] px-1.5 py-px text-[10px] text-white/35">
                    已停用
                  </span>
                )}
                <span className="flex-1" />
                <span className="shrink-0 text-[10px] text-white/25">
                  {r.model_count} 個模型
                </span>
              </div>
            ))}
          </div>
          <p className="pt-2.5 text-[11px] leading-relaxed text-white/25">
            {cur ? `${cur.model}：` : ""}
            候選 {rows.data?.length ?? 0} 個來源。實際請求還會逐來源試「宣告協議 → 學到的協議
            → 另一種協議」（Responses 入站時 chat ↔ responses 都生得出來），
            這一層由網關自動決定，成功後記在資料庫，重啟仍有效。
          </p>
        </>
      )}
    </div>
  );
}
