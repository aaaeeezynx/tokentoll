//! 由 `ModelCatalog.tsx` 拆分而來（原檔 613 行）。程式碼語意未改，只搬位置。

import {
  useEffect,
  useState,
} from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  api,
  type Provider,
  type ProviderPricing,
} from "../../../lib/api";
import {
  Icon,
} from "../../icons";
import {
  PopSelect,
} from "../../PopSelect";
import {
  useConfirm,
} from "../../Confirm";
import {
  ChannelPricingForm,
} from "../ChannelPricing";
import {
  MODE_LABEL,
} from "../SwitchDialog";

export function ChannelPricing(props: {
  providers: Provider[];
  focusPid?: number | null;
}) {
  const qc = useQueryClient();
  const [pid, setPid] = useState<number | null>(props.focusPid ?? null);
  useEffect(() => {
    if (props.focusPid != null) setPid(props.focusPid);
  }, [props.focusPid]);
  const [editing, setEditing] = useState<ProviderPricing | "new" | null>(
    null,
  );
  const effectivePid = pid ?? props.providers[0]?.id ?? null;
  const rows = useQuery({
    queryKey: ["pp", effectivePid],
    queryFn: () => api.ppList(effectivePid),
    enabled: effectivePid != null,
  });

  const refresh = () =>
    void qc.invalidateQueries({ queryKey: ["pp", effectivePid] });
  const del = useMutation({
    mutationFn: ({ p, m }: { p: number; m: string }) => api.ppDelete(p, m),
    onSuccess: refresh,
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <PopSelect
          className="min-w-[160px] flex-1"
          value={effectivePid != null ? String(effectivePid) : ""}
          onChange={(v) => setPid(Number(v))}
          options={props.providers.map((p) => ({
            value: String(p.id),
            label: p.name,
            hint: p.base_url,
          }))}
        />
        <button
          className="btn-ghost flex shrink-0 items-center gap-1.5 px-3.5 py-1.5 text-[13px]"
          onClick={() => {
            setEditing("new");
          }}
        >
          <Icon name="plus" size={12} />
          模型定價
        </button>
      </div>
      <div className="mac-rows scroll-y max-h-[38vh]">
        {(rows.data || []).map((r) => {
          const key = `${r.provider_id}:${r.model_norm}`;
          const open =
            editing !== null &&
            editing !== "new" &&
            `${editing.provider_id}:${editing.model_norm}` === key;
          return (
            <div
              key={key}
              className="mac-row clickable"
              data-open={open}
              onClick={() =>
                setEditing((e) =>
                  e !== null &&
                  e !== "new" &&
                  `${e.provider_id}:${e.model_norm}` === key
                    ? null
                    : r,
                )
              }
            >
              <div className="flex items-center gap-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate font-mono text-[13px] text-fg/85">
                    {r.model_norm === "*" ? "*（該來源預設）" : r.model_norm}
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-fg/30">
                    基礎 {r.in_pm}/{r.out_pm}
                    {r.mode === "subscription" &&
                      ` · 月費 $${r.sub_fee_usd} · 已用 ${r.used_this_month.toLocaleString()}`}
                    {r.mode === "tou" &&
                      ` · 峰谷 ${r.periods.length} 時段`}
                  </div>
                  {r.mode === "subscription" &&
                    r.sub_included_tokens >= 0 && (
                      <div className="mt-1.5 h-1 w-24 overflow-hidden rounded-full bg-fg/[0.06]">
                        <div
                          className="h-full rounded-full bg-(--mac-accent)/50 bar-anim"
                          style={{
                            width: `${Math.min(100, (r.used_this_month / Math.max(1, r.sub_included_tokens)) * 100)}%`,
                          }}
                        />
                      </div>
                    )}
                </div>
                <span className="shrink-0 rounded-full bg-fg/[0.06] px-2 py-0.5 text-[10px] font-medium text-fg/40">
                  {MODE_LABEL[r.mode]}
                </span>
                <button
                  className="shrink-0 rounded-full p-1.5 text-fg/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
                  title="刪除"
                  onClick={(e) => {
                    e.stopPropagation();
                    ask(`刪除 ${r.model_norm} 的來源定價？`, () =>
                      del.mutate({ p: r.provider_id, m: r.model_norm }),
                    );
                  }}
                >
                  <Icon name="trash" size={13} />
                </button>
                <Icon
                  name="chevron-down"
                  size={12}
                  className={`shrink-0 text-fg/25 transition-transform ${open ? "rotate-180" : ""}`}
                />
              </div>
              <div className="acc-body" data-open={open}>
            <div>
                  <div
                    className="mt-2.5 border-t border-fg/[0.06] pt-1"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <ChannelPricingForm
                      key={key}
                      providerId={r.provider_id}
                      initial={editing !== null && editing !== "new" ? editing : null}
                      onDone={() => {
                        setEditing(null);
                        refresh();
                      }}
                      onCancel={() => setEditing(null)}
                    />
                  </div>
                </div>
              </div>
            </div>
          );
        })}
        {(rows.data || []).length === 0 && (
          <div className="mac-row text-xs text-fg/20">
            該來源還沒有單獨定價，會回退到全域預設定價（見下方）。
          </div>
        )}
      </div>
      {editing === "new" && effectivePid != null && (
        <div className="mac-rows mt-2 p-3.5">
          <div className="mb-1 text-[13px] font-semibold text-fg/80">
            新增模型定價
          </div>
          <ChannelPricingForm
            providerId={effectivePid}
            initial={null}
            onDone={() => {
              setEditing(null);
              refresh();
            }}
            onCancel={() => setEditing(null)}
          />
        </div>
      )}
      {confirmDialog}
    </div>
  );
}
