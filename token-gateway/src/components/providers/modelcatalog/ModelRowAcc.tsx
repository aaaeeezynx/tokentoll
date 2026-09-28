//! 由 `ModelCatalog.tsx` 拆分而來（原檔 613 行）。程式碼語意未改，只搬位置。

import {
  useEffect,
  useState,
} from "react";
import {
  useMutation,
} from "@tanstack/react-query";
import {
  api,
  REASONING_OPTIONS,
  type UsableModel,
  type UsableModelInput,
} from "../../../lib/api";
import {
  Icon,
} from "../../icons";
import {
  PopSelect,
} from "../../PopSelect";
import {
  Toggle,
} from "../../Toggle";
import {
  useConfirm,
} from "../../Confirm";

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
