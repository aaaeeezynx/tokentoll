//! 由 `Providers.tsx` 拆分而來（原檔 2,845 行）。程式碼語意未改，只搬位置。

import {
  useState,
} from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  api,
  type ProviderPricing,
  type ProviderPricingInput,
  type PricingMode,
  type PricingPeriod,
  type PricingPeriodInput,
} from "../../lib/api";
import {
  Icon,
} from "../icons";
import {
  PopSelect,
} from "../PopSelect";
import {
  useConfirm,
} from "../Confirm";

export function ChannelPricingForm(props: {
  providerId: number;
  initial: ProviderPricing | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [model, setModel] = useState(props.initial?.model_norm ?? "");
  const [mode, setMode] = useState<PricingMode>(
    props.initial?.mode ?? "usage",
  );
  const [rates, setRates] = useState({
    in_pm: props.initial?.in_pm ?? 0,
    out_pm: props.initial?.out_pm ?? 0,
    cache_read_pm: props.initial?.cache_read_pm ?? 0,
    cache_create_pm: props.initial?.cache_create_pm ?? 0,
  });
  const [subFee, setSubFee] = useState(props.initial?.sub_fee_usd ?? 0);
  const [subIncluded, setSubIncluded] = useState(
    props.initial?.sub_included_tokens ?? -1,
  );
  const [err, setErr] = useState("");

  const save = useMutation({
    mutationFn: () => {
      const input: ProviderPricingInput = {
        provider_id: props.providerId,
        model_norm: model.trim(),
        mode,
        in_pm: rates.in_pm,
        out_pm: rates.out_pm,
        cache_read_pm: rates.cache_read_pm,
        cache_create_pm: rates.cache_create_pm,
        sub_fee_usd: subFee,
        sub_included_tokens: subIncluded,
      };
      return api.ppUpsert(input);
    },
    onSuccess: props.onDone,
    onError: (e) => setErr(String(e)),
  });

  const inputCls =
    "field w-full px-3 py-1.5 text-xs";
  const num = (v: string) => Number(v) || 0;
  return (
    <div>
      <div className="mac-frow">
        <span className="mac-cap">模型 ID</span>
        <input
          className={inputCls}
          placeholder="* 表示該來源預設"
          value={model}
          onChange={(e) => setModel(e.target.value)}
        />
      </div>
      <div className="mac-frow">
        <span className="mac-cap">計費模式</span>
        <PopSelect
          value={mode}
          onChange={(v) => setMode(v as PricingMode)}
          options={[
            { value: "usage", label: "按量計費" },
            { value: "subscription", label: "訂閱制" },
            { value: "tou", label: "峰谷計費" },
          ]}
        />
      </div>
      {(
        [
          ["in_pm", "輸入/百萬"],
          ["out_pm", "輸出/百萬"],
          ["cache_read_pm", "快取讀/百萬"],
          ["cache_create_pm", "快取建/百萬"],
        ] as const
      ).map(([k, label]) => (
        <div key={k} className="mac-frow">
          <span className="mac-cap">
            {mode === "subscription" ? `超額${label}` : label}
          </span>
          <input
            className={inputCls}
            value={rates[k]}
            onChange={(e) =>
              setRates((r) => ({ ...r, [k]: num(e.target.value) }))
            }
          />
        </div>
      ))}
      {mode === "subscription" && (
        <>
          <div className="mac-frow">
            <span className="mac-cap">月費 $</span>
            <input
              className={inputCls}
              value={subFee}
              onChange={(e) => setSubFee(num(e.target.value))}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">免費額度</span>
            <input
              className={inputCls}
              placeholder="tokens，-1 無限"
              value={subIncluded}
              onChange={(e) => setSubIncluded(Number(e.target.value) || 0)}
            />
          </div>
        </>
      )}
      {mode === "tou" && (
        <div className="mt-2 rounded-xl border border-fg/[0.06] bg-black/20 p-2.5">
          <div className="mb-1.5 text-[11px] font-medium text-fg/45">
            峰谷時段（未命中用基礎價，可跨夜）
          </div>
          {model.trim() ? (
            <PeriodManager
              providerId={props.providerId}
              modelNorm={model.trim()}
            />
          ) : (
            <p className="text-[11px] text-amber-400">
              請先填寫上方模型 ID（`*` 表示該來源預設）
            </p>
          )}
        </div>
      )}
      {err && <p className="pt-1 text-xs text-red-400">{err}</p>}
      <div className="flex justify-end gap-2 pt-2.5">
        <button
          className="btn-ghost px-4 py-1.5 text-xs"
          onClick={props.onCancel}
        >
          取消
        </button>
        <button
          className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
          disabled={save.isPending}
          onClick={() => save.mutate()}
        >
          儲存
        </button>
      </div>
    </div>
  );
}

export function PeriodManager(props: { providerId: number; modelNorm: string }) {
  const qc = useQueryClient();
  const list = useQuery({
    queryKey: ["periods", props.providerId, props.modelNorm],
    queryFn: () => api.ppPeriods(props.providerId, props.modelNorm),
  });
  const [editing, setEditing] = useState<PricingPeriod | "new" | null>(null);
  const [name, setName] = useState("");
  const [start, setStart] = useState("09:00");
  const [end, setEnd] = useState("18:00");
  const [ord, setOrd] = useState(0);
  const [rates, setRates] = useState({ in: 0, out: 0, cr: 0, cw: 0 });
  const [err, setErr] = useState("");

  const openNew = () => {
    setName("");
    setStart("09:00");
    setEnd("18:00");
    setOrd((list.data?.length ?? 0) * 10);
    setRates({ in: 0, out: 0, cr: 0, cw: 0 });
    setErr("");
    setEditing("new");
  };
  const openEdit = (p: PricingPeriod) => {
    setName(p.name);
    setStart(p.start);
    setEnd(p.end);
    setOrd(p.ord);
    setRates({ in: p.in_pm, out: p.out_pm, cr: p.cache_read_pm, cw: p.cache_create_pm });
    setErr("");
    setEditing(p);
  };

  const refresh = () => {
    void qc.invalidateQueries({
      queryKey: ["periods", props.providerId, props.modelNorm],
    });
    void qc.invalidateQueries({ queryKey: ["pp"] });
  };
  const { dialog: confirmDialog, ask } = useConfirm();
  const save = useMutation({
    mutationFn: async (): Promise<void> => {
      const input: PricingPeriodInput = {
        provider_id: props.providerId,
        model_norm: props.modelNorm,
        name: name.trim(),
        start: start.trim(),
        end: end.trim(),
        ord,
        in_pm: rates.in,
        out_pm: rates.out,
        cache_read_pm: rates.cr,
        cache_create_pm: rates.cw,
      };
      if (editing !== null && editing !== "new") {
        await api.ppPeriodUpdate(editing.id, input);
      } else {
        await api.ppPeriodAdd(input);
      }
    },
    onSuccess: () => {
      setEditing(null);
      refresh();
    },
    onError: (e) => setErr(String(e)),
  });
  const del = useMutation({
    mutationFn: (id: number) => api.ppPeriodDelete(id),
    onSuccess: refresh,
  });

  const inputCls =
    "field w-full px-3 py-1.5 text-xs";
  const num = (v: string) => Number(v) || 0;
  return (
    <div>
      <div className="mac-rows">
        {(list.data || []).map((p) => (
          <div key={p.id} className="mac-row !py-2">
            <div className="flex items-center gap-2 text-xs">
              <div className="min-w-0 flex-1 truncate text-fg/70">
                {p.name}
                <span className="ml-1.5 font-mono text-fg/35">
                  {p.start}-{p.end} · {p.in_pm}/{p.out_pm}
                </span>
              </div>
              <button
                className="shrink-0 rounded-full p-1 text-fg/40 transition-colors hover:bg-fg/10 hover:text-fg"
                title="編輯時段"
                onClick={() => openEdit(p)}
              >
                <Icon name="pencil" size={12} />
              </button>
              <button
                className="shrink-0 rounded-full p-1 text-fg/40 transition-colors hover:bg-red-500/10 hover:text-red-400"
                title="刪除時段"
                onClick={() => ask(`刪除時段 ${p.name}？`, () => del.mutate(p.id))}
              >
                <Icon name="trash" size={12} />
              </button>
            </div>
          </div>
        ))}
        {(list.data || []).length === 0 && !editing && (
          <div className="mac-row !py-2 text-[11px] text-fg/20">
            尚無時段
          </div>
        )}
      </div>
      {editing ? (
        <div className="mac-rows mt-2 p-3">
          <div className="mac-frow">
            <span className="mac-cap">名稱</span>
            <input
              className={inputCls}
              placeholder="如 早峰"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">開始–結束</span>
            <div className="flex items-center gap-1.5">
              <input
                className={inputCls}
                placeholder="HH:MM"
                value={start}
                onChange={(e) => setStart(e.target.value)}
              />
              <span className="text-fg/25">–</span>
              <input
                className={inputCls}
                placeholder="HH:MM"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
              />
            </div>
          </div>
          <div className="mac-frow">
            <span className="mac-cap">優先級</span>
            <input
              className={inputCls}
              value={ord}
              onChange={(e) => setOrd(Number(e.target.value) || 0)}
            />
          </div>
          {(
            [
              ["in", "輸入/百萬"],
              ["out", "輸出/百萬"],
              ["cr", "快取讀/百萬"],
              ["cw", "快取建/百萬"],
            ] as const
          ).map(([k, label]) => (
            <div key={k} className="mac-frow">
              <span className="mac-cap">{label}</span>
              <input
                className={inputCls}
                value={rates[k]}
                onChange={(e) =>
                  setRates((r) => ({ ...r, [k]: num(e.target.value) }))
                }
              />
            </div>
          ))}
          {err && <p className="pt-1 text-[11px] text-red-400">{err}</p>}
          <div className="flex justify-end gap-1.5 pt-2">
            <button
              className="btn-ghost px-3 py-1 text-xs"
              onClick={() => setEditing(null)}
            >
              取消
            </button>
            <button
              className="btn-primary px-3 py-1 text-xs"
              onClick={() => save.mutate()}
            >
              儲存
            </button>
          </div>
        </div>
      ) : (
        <button
          className="btn-ghost mt-2 flex items-center gap-1 px-2.5 py-1 text-xs"
          onClick={openNew}
        >
          <Icon name="plus" size={11} />
          時段
        </button>
      )}
      <p className="mt-1.5 text-[10px] text-fg/15">
        重疊時取優先級（ord）最小命中的；`*` 模型時段為全來源回退。
      </p>
      {confirmDialog}
    </div>
  );
}
