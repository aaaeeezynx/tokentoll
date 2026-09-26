//! 由 `Providers.tsx` 拆分而來（原檔 2,845 行）。程式碼語意未改，只搬位置。

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
  REASONING_OPTIONS,
  type Provider,
  type ProviderPricing,
  type UsableModel,
  type UsableModelInput,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  PopSelect,
} from "../PopSelect";
import {
  Toggle,
} from "../Toggle";
import {
  useConfirm,
} from "../Confirm";
import {
  ChannelPricingForm,
} from "./ChannelPricing";
import {
  MODE_LABEL,
} from "./SwitchDialog";

export function fmtCatalogTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}

/** 模型行：唯讀摘要＋點選展開編輯（macOS inspector 手風琴）。 */
export function ModelRowAcc(props: {
  row: UsableModel;
  catalogIds: string[];
  open: boolean;
  onToggle: () => void;
  onChanged: () => void;
  onMsg: (s: string) => void;
}) {
  const { row, catalogIds, open } = props;
  const [custom, setCustom] = useState(
    catalogIds.length > 0 && !catalogIds.includes(row.actual_model),
  );
  const [display, setDisplay] = useState(row.display_name);
  const [actual, setActual] = useState(row.actual_model);
  const [context, setContext] = useState(
    row.context_window != null ? String(row.context_window) : "",
  );
  const [reasoning, setReasoning] = useState<string>(
    row.reasoning,
  );
  // 展開時用最新值重置草稿
  useEffect(() => {
    if (open) {
      setDisplay(row.display_name);
      setActual(row.actual_model);
      setContext(row.context_window != null ? String(row.context_window) : "");
      setReasoning(row.reasoning);
      setCustom(catalogIds.length > 0 && !catalogIds.includes(row.actual_model));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open ]);

  const inputCls = "field w-full px-3 py-1.5 text-xs";

  const save = useMutation({
    mutationFn: (input: UsableModelInput) => api.modelUpdate(row.id, input),
    onSuccess: props.onChanged,
    onError: (e) => props.onMsg(String(e)),
  });
  const del = useMutation({
    mutationFn: () => api.modelDelete(row.id),
    onSuccess: props.onChanged,
    onError: (e) => props.onMsg(String(e)),
  });
  const { dialog: confirmDialog, ask } = useConfirm();
  const base: UsableModelInput = {
    provider_id: row.provider_id,
    display_name: row.display_name,
    actual_model: row.actual_model,
    context_window: row.context_window,
    reasoning: row.reasoning,
    ord: row.ord,
    enabled: row.enabled,
  };
  const commit = () => {
    const a = actual.trim();
    if (!a) {
      props.onMsg("實際請求模型不能為空");
      return;
    }
    save.mutate(
      {
        ...base,
        display_name: display.trim() || a,
        actual_model: a,
        context_window: context.trim() === "" ? null : Number(context) || 0,
        reasoning,
      },
      {
        // 表單儲存成功後自動收起（開關啟用走同一 mutation，不收起）
        onSuccess: () => {
          if (open) props.onToggle();
        },
      },
    );
  };

  return (
    <div
      className="mac-row clickable no-hover"
      data-open={open}
      onClick={props.onToggle}
    >
      <div className="flex items-center gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium text-white/85">
            {row.display_name}
          </div>
          <div className="mt-0.5 truncate font-mono text-[11px] text-white/30">
            {row.actual_model}
            {row.context_window ? ` · ctx ${row.context_window.toLocaleString()}` : ""}
            {row.reasoning && row.reasoning !== "unset" ? ` · ${row.reasoning.includes(",") ? row.reasoning.split(",").join(" / ") : row.reasoning}` : ""}
            {!row.enabled && <span className="ml-1.5 text-white/20">· 已停用</span>}
          </div>
        </div>
        <Toggle
          size="sm"
          checked={row.enabled}
          title="啟用"
          onChange={(v) => save.mutate({ ...base, enabled: v })}
        />
        <button
          className="shrink-0 rounded-full p-1.5 text-white/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="刪除"
          onClick={(e) => {
            e.stopPropagation();
            ask(`刪除映射 ${row.display_name}？`, () => del.mutate(), {
              message: "該模型的可用映射將被移除，不影響已抓取的目錄。",
            });
          }}
        >
          <Icon name="trash" size={13} />
        </button>
        <Icon
          name="chevron-down"
          size={12}
          className={`shrink-0 text-white/25 transition-transform ${open ? "rotate-180" : ""}`}
        />
      </div>
      <div className="acc-body" data-open={open}>
        <div>
          <div
            className="mt-2.5 border-t border-white/[0.06] pt-1"
            onClick={(e) => e.stopPropagation()}
          >
          <div className="mac-frow">
            <span className="mac-cap">選單顯示名</span>
            <input
              className={inputCls}
              placeholder="空=同實際模型"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">實際請求模型</span>
            {custom ? (
              <div className="flex gap-1.5">
                <input
                  className={inputCls}
                  placeholder="模型 ID"
                  value={actual}
                  onChange={(e) => setActual(e.target.value)}
                />
                <button
                  className="btn-ghost flex shrink-0 items-center px-2 py-1 text-xs"
                  title="改用清單選擇"
                  onClick={() => setCustom(false)}
                >
                  <Icon name="chevron-left" size={12} />
                </button>
              </div>
            ) : (
              <PopSelect
                searchable
                value={
                  catalogIds.includes(actual) ? actual : "__custom"
                }
                onChange={(v) => {
                  if (v === "__custom") {
                    setCustom(true);
                    return;
                  }
                  setActual(v);
                }}
                options={[
                  ...catalogIds.map((id) => ({ value: id, label: id })),
                  { value: "__custom", label: "自訂輸入…" },
                ]}
              />
            )}
          </div>
          <div className="mac-frow">
            <span className="mac-cap">上下文視窗</span>
            <input
              className={inputCls}
              placeholder="例如 128000（空=不寫）"
              value={context}
              onChange={(e) => setContext(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">思考等級<span className="ml-1 font-normal text-white/25">空 = 全部檔位</span></span>
            <PopSelect
              multi
              value={reasoning}
              onChange={setReasoning}
              options={REASONING_OPTIONS.filter((o) => o.id !== "unset").map((o) => ({
                value: o.id,
                label: o.label,
              }))}
            />
          </div>
          <div className="flex gap-2 pt-2.5">
            <button
              className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
              disabled={save.isPending}
              onClick={commit}
            >
              儲存
            </button>
          </div>
          </div>
        </div>
      </div>
      {confirmDialog}
    </div>
  );
}

export function ModelPanel(props: { provider: Provider; rowsClassName?: string; apiKeyOverride?: string; baseUrlOverride?: string }) {
  const qc = useQueryClient();
  const pid = props.provider.id;
  const catalog = useQuery({
    queryKey: ["catalog", pid],
    queryFn: () => api.catalogGet(pid),
    retry: false,
  });
  const rows = useQuery({
    queryKey: ["umodels", pid],
    queryFn: () => api.modelsList(pid),
  });
  const [adding, setAdding] = useState(false);
  const [actual, setActual] = useState("");
  const [display, setDisplay] = useState("");
  const [context, setContext] = useState("");
  const [reasoning, setReasoning] = useState<string>("");
  const [msg, setMsg] = useState("");

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["umodels", pid] });
    void qc.invalidateQueries({ queryKey: ["model_counts"] });
  };
  const fetch = useMutation({
    mutationFn: () => api.catalogFetch(pid, props.apiKeyOverride, props.baseUrlOverride),
    onSuccess: (c) => {
      setMsg(`已取得 ${c.models.length} 個模型`);
      void qc.invalidateQueries({ queryKey: ["catalog", pid] });
    },
    onError: (e) => setMsg(String(e)),
  });
  const add = useMutation({
    mutationFn: () => {
      const a = actual.trim();
      const input: UsableModelInput = {
        provider_id: pid,
        display_name: display.trim() || a,
        actual_model: a,
        context_window:
          context.trim() === "" ? null : Number(context) || 0,
        reasoning,
        ord: (rows.data?.length ?? 0) * 10,
        enabled: true,
      };
      return api.modelAdd(input);
    },
    onSuccess: () => {
      setAdding(false);
      setActual("");
      setDisplay("");
      setContext("");
      setReasoning("");
      setMsg("");
      refresh();
    },
    onError: (e) => setMsg(String(e)),
  });

  const catalogIds = (catalog.data?.models || []).map((m) => m.id);
  const [openId, setOpenId] = useState<number | null>(null);
  const inputClsSm = "field w-full px-3 py-1.5 text-xs";
  return (
    <div>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <button
          className="btn-ghost flex items-center gap-1.5 px-3.5 py-1.5 text-[13px] disabled:opacity-40"
          disabled={fetch.isPending}
          onClick={() => fetch.mutate()}
        >
          <Icon name="download" size={13} />
          取得模型清單
        </button>
        <button
          className="btn-ghost flex items-center gap-1.5 px-3.5 py-1.5 text-[13px]"
          onClick={() => {
            setActual(catalogIds[0] ?? "");
            setDisplay("");
            setContext("");
            setReasoning("");
            setOpenId(null);
            setAdding(true);
          }}
        >
          <Icon name="plus" size={12} />
          新增模型
        </button>
        <span className="ml-auto text-[11px] text-white/25">
          {catalog.data
            ? `共 ${catalog.data.models.length} 個 · 更新於 ${fmtCatalogTime(catalog.data.fetched_at)}`
            : "尚未抓取"}
        </span>
      </div>
      {!props.provider.api_key && !props.apiKeyOverride && (
        <p className="mb-2 text-xs text-amber-400">
          該來源尚未填寫 API Key（請在上方表單填入），否則抓取會 401/403
        </p>
      )}
      {msg && <p className="mb-2 text-xs text-amber-300">{msg}</p>}
      <div className={`mac-rows scroll-y ${props.rowsClassName ?? "max-h-[38vh]"}`}>
        {(rows.data || []).map((r) => (
          <ModelRowAcc
            key={r.id}
            row={r}
            catalogIds={catalogIds}
            open={openId === r.id}
            onToggle={() => setOpenId((o) => (o === r.id ? null : r.id))}
            onChanged={refresh}
            onMsg={setMsg}
          />
        ))}
        {(rows.data || []).length === 0 && (
          <div className="mac-row text-xs text-white/20">
            暫無使用模型，點「新增模型」建立（修改後需重啟對應 CLI 生效）。
          </div>
        )}
      </div>
      {adding && (
        <div className="mac-rows mt-2 p-3.5">
          <div className="mb-1 text-[13px] font-semibold text-white/80">
            新增模型
          </div>
          <div className="mac-frow">
            <span className="mac-cap">選單顯示名</span>
            <input
              className={inputClsSm}
              placeholder="空=同實際模型"
              value={display}
              onChange={(e) => setDisplay(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">實際請求模型</span>
            {catalogIds.length > 0 ? (
              <PopSelect
                searchable
                value={actual}
                onChange={setActual}
                options={catalogIds.map((id) => ({ value: id, label: id }))}
              />
            ) : (
              <input
                className={inputClsSm}
                placeholder="實際請求模型 ID"
                value={actual}
                onChange={(e) => setActual(e.target.value)}
              />
            )}
          </div>
          <div className="mac-frow">
            <span className="mac-cap">上下文視窗</span>
            <input
              className={inputClsSm}
              placeholder="空=不寫"
              value={context}
              onChange={(e) => setContext(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">思考等級<span className="ml-1 font-normal text-white/25">空 = 全部檔位</span></span>
            <PopSelect
              multi
              value={reasoning}
              onChange={setReasoning}
              options={REASONING_OPTIONS.filter((o) => o.id !== "unset").map((o) => ({
                value: o.id,
                label: o.label,
              }))}
            />
          </div>
          <div className="flex justify-end gap-2 pt-2.5">
            <button
              className="btn-ghost px-4 py-1.5 text-xs"
              onClick={() => setAdding(false)}
            >
              取消
            </button>
            <button
              className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
              disabled={add.isPending}
              onClick={() => add.mutate()}
            >
              儲存
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

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
                  <div className="truncate font-mono text-[13px] text-white/85">
                    {r.model_norm === "*" ? "*（該來源預設）" : r.model_norm}
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-white/30">
                    基礎 {r.in_pm}/{r.out_pm}
                    {r.mode === "subscription" &&
                      ` · 月費 $${r.sub_fee_usd} · 已用 ${r.used_this_month.toLocaleString()}`}
                    {r.mode === "tou" &&
                      ` · 峰谷 ${r.periods.length} 時段`}
                  </div>
                  {r.mode === "subscription" &&
                    r.sub_included_tokens >= 0 && (
                      <div className="mt-1.5 h-1 w-24 overflow-hidden rounded-full bg-white/[0.06]">
                        <div
                          className="h-full rounded-full bg-(--mac-accent)/50 bar-anim"
                          style={{
                            width: `${Math.min(100, (r.used_this_month / Math.max(1, r.sub_included_tokens)) * 100)}%`,
                          }}
                        />
                      </div>
                    )}
                </div>
                <span className="shrink-0 rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-medium text-white/40">
                  {MODE_LABEL[r.mode]}
                </span>
                <button
                  className="shrink-0 rounded-full p-1.5 text-white/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
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
                  className={`shrink-0 text-white/25 transition-transform ${open ? "rotate-180" : ""}`}
                />
              </div>
              <div className="acc-body" data-open={open}>
            <div>
                  <div
                    className="mt-2.5 border-t border-white/[0.06] pt-1"
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
          <div className="mac-row text-xs text-white/20">
            該來源還沒有單獨定價，會回退到全域預設定價（見下方）。
          </div>
        )}
      </div>
      {editing === "new" && effectivePid != null && (
        <div className="mac-rows mt-2 p-3.5">
          <div className="mb-1 text-[13px] font-semibold text-white/80">
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
