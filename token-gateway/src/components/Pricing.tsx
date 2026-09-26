import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type PricingInput } from "../lib/api";
import { Icon } from "./icons";
import { useConfirm } from "./Confirm";

/** 全域預設定價（來源未單獨定價時的回退；種子不可刪只能覆蓋）。 */
export default function GlobalPricingManager(props: {
  onCount?: (n: number) => void;
}) {
  const qc = useQueryClient();
  const list = useQuery({ queryKey: ["pricing"], queryFn: api.pricingList });
  const [form, setForm] = useState<PricingInput>({
    model_norm: "",
    in_pm: 0,
    out_pm: 0,
    cache_read_pm: 0,
    cache_create_pm: 0,
  });
  const [err, setErr] = useState("");
  const [adding, setAdding] = useState(false);

  const refresh = () =>
    void qc.invalidateQueries({ queryKey: ["pricing"] });

  const save = useMutation({
    mutationFn: () => api.pricingUpsert(form),
    onSuccess: () => {
      setForm({
        model_norm: "",
        in_pm: 0,
        out_pm: 0,
        cache_read_pm: 0,
        cache_create_pm: 0,
      });
      setErr("");
      setAdding(false);
      refresh();
    },
    onError: (e) => setErr(String(e)),
  });
  const del = useMutation({
    mutationFn: (m: string) => api.pricingDelete(m),
    onSuccess: refresh,
  });
  const clearAll = useMutation({
    mutationFn: api.pricingClear,
    onSuccess: refresh,
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  useEffect(() => {
    props.onCount?.(list.data?.length ?? 0);
  }, [list.data, props]);

  const inputCls = "field w-full px-3 py-1.5 text-xs";
  return (
    <div>
      <div className="mb-3 flex items-center gap-2">
        <button
          className="btn-ghost flex items-center gap-1.5 px-3.5 py-1.5 text-[13px]"
          onClick={() => setAdding((a) => !a)}
        >
          <Icon name="plus" size={12} />
          新增定價
        </button>
        <span className="text-[11px] text-white/25">
          美元/百萬 tokens，0 表示免費
        </span>
        <span className="flex-1" />
        <button
          className="btn-danger-ghost px-3 py-1 text-xs disabled:opacity-40"
          disabled={clearAll.isPending}
          onClick={() =>
            ask("清空全部全域預設定價？", () => clearAll.mutate(), {
              message: "重啟不會恢復，來源定價不受影響。",
              confirmLabel: "清空",
            })
          }
        >
          清空全部
        </button>
      </div>
      {adding && (
        <div className="mac-rows mb-3 p-3.5">
          <div className="mac-frow">
            <span className="mac-cap">模型 ID</span>
            <input
              className={inputCls}
              placeholder="小寫，如 deepseek-v4-flash"
              value={form.model_norm}
              onChange={(e) =>
                setForm((f) => ({ ...f, model_norm: e.target.value }))
              }
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
              <span className="mac-cap">{label}</span>
              <input
                className={inputCls}
                value={form[k]}
                onChange={(e) =>
                  setForm((f) => ({ ...f, [k]: Number(e.target.value) || 0 }))
                }
              />
            </div>
          ))}
          {err && <p className="pt-1 text-xs text-red-400">{err}</p>}
          <div className="flex justify-end gap-2 pt-2.5">
            <button
              className="btn-ghost px-4 py-1.5 text-xs"
              onClick={() => {
                setAdding(false);
                setErr("");
              }}
            >
              取消
            </button>
            <button
              className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
              disabled={save.isPending}
              onClick={() => save.mutate()}
            >
              新增 / 覆蓋
            </button>
          </div>
        </div>
      )}
      {err && !adding && <p className="mb-2 text-xs text-red-400">{err}</p>}
      <div className="mac-rows scroll-y max-h-[38vh]">
        {(list.data || []).map((p) => (
          <div key={p.model_norm} className="mac-row">
            <div className="flex items-center gap-2">
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-[13px] text-white/85">
                  {p.model_norm}
                </div>
                <div className="mt-0.5 truncate text-[11px] text-white/30">
                  {p.in_pm}/{p.out_pm}/{p.cache_read_pm}/{p.cache_create_pm}
                  <span className="ml-1.5 rounded-full bg-white/[0.06] px-1.5 py-px text-[10px] text-white/35">
                    {p.source}
                  </span>
                </div>
              </div>
              <button
                className="shrink-0 rounded-full p-1.5 text-white/35 transition-colors hover:bg-red-500/10 hover:text-red-400"
                title="刪除"
                onClick={() =>
                  ask(`刪除 ${p.model_norm} 的定價？`, () =>
                    del.mutate(p.model_norm),
                  )
                }
              >
                <Icon name="trash" size={13} />
              </button>
            </div>
          </div>
        ))}
        {(list.data || []).length === 0 && (
          <div className="mac-row text-xs text-white/20">
            暫無全域定價，未命中時費用記 0。
          </div>
        )}
      </div>
      {confirmDialog}
    </div>
  );
}
