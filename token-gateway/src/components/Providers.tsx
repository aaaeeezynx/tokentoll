import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  useGatewayPort,
  type Provider,
  type ToolStatus,
} from "../lib/api";
import { Icon } from "./icons";
import { Segmented } from "./Segmented";
import { useConfirm } from "./Confirm";
import GlobalPricingManager from "./Pricing";
import { ProviderSheet, ToolVersions } from "./providers/ProviderForm";
import { FailoverQueue } from "./providers/FailoverQueue";
import { ProvidersIO } from "./providers/ProvidersIO";
import { SourceCard } from "./providers/SourceCard";
import { SourceDetail } from "./providers/SourceDetail";
import { ToolList } from "./providers/ToolList";
import { SwitchDialog } from "./providers/SwitchDialog";
import { useDragSort } from "./providers/useDragSort";
import { useToolSwitch } from "./providers/useToolSwitch";

export default function ProvidersPage() {
  const qc = useQueryClient();
  const port = useGatewayPort();
  const providers = useQuery({
    queryKey: ["providers"],
    queryFn: api.providersList,
  });
  const tools = useQuery({
    queryKey: ["tools", port],
    queryFn: () => api.toolsDetect(port),
    refetchInterval: 10000,
  });
  const [editing, setEditing] = useState<Provider | "new" | null>(null);
  const [gpCount, setGpCount] = useState<number | null>(null);
  const [gpOpen, setGpOpen] = useState(false);
  const [view, setView] = useState<"sources" | "tools">("sources");
  const [switching, setSwitching] = useState<ToolStatus | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [detailTab, setDetailTab] = useState<"models" | "pricing">("models");
  const counts = useQuery({
    queryKey: ["model_counts"],
    queryFn: api.modelCounts,
  });
  const countMap = useMemo(
    () => new Map((counts.data || []).map(([pid, n]) => [pid, n] as const)),
    [counts.data],
  );
  const selId = selected ?? providers.data?.[0]?.id ?? null;
  const sel = (providers.data || []).find((p) => p.id === selId) ?? null;

  const del = useMutation({
    mutationFn: (id: number) => api.providerDelete(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["providers"] }),
  });
  /** 複製來源：連模型／定價／時段一起，複製完直接選中新的那一筆。 */
  const dup = useMutation({
    mutationFn: (id: number) => api.providerDuplicate(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["providers"] });
      void qc.invalidateQueries({ queryKey: ["model_counts"] });
    },
  });
  const { dialog: confirmDialog, ask } = useConfirm();

  const [listMsg, setListMsg] = useState("");

  const [doctorLines, setDoctorLines] = useState<string[] | null>(null);
  const [doctorBusy, setDoctorBusy] = useState(false);

  /** 提交全排列（拖拽落點 / 未來鍵盤排序共用）。 */
  const commitOrder = async (ids: number[]) => {
    try {
      await api.providerReorder(ids);
      void qc.invalidateQueries({ queryKey: ["providers"] });
    } catch (e) {
      setListErr(String(e));
      void qc.invalidateQueries({ queryKey: ["providers"] }); // 失敗回退到庫內真序
    }
  };
  // 來源拖拽排序：邏輯抽到 useDragSort（見 providers/useDragSort.ts）。
  const {
    dragId,
    drop,
    listErr,
    setListErr,
    listRef,
    gripDown,
    suppressClick,
  } = useDragSort(
    (providers.data || []).map((p) => p.id),
    commitOrder,
  );

  const runDoctor = () => {
    setDoctorBusy(true);
    api
      .codexDoctor(port)
      .then((lines) => setDoctorLines(lines))
      .catch((e) => setDoctorLines([`❌ 體檢執行失敗：${String(e)}`]))
      .finally(() => setDoctorBusy(false));
  };

  const refreshTools = () => {
    void qc.invalidateQueries({ queryKey: ["tools", port] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
  };

  // 接管／還原行為抽到 useToolSwitch（見 providers/useToolSwitch.ts）。
  const { toggling, rowErr, rowMsg, toggleRow } = useToolSwitch({
    port,
    providers: providers.data || [],
    selId,
    tools: tools.data || [],
    refreshTools,
  });

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["providers"] });
    void qc.invalidateQueries({ queryKey: ["tools", port] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
    void qc.invalidateQueries({ queryKey: ["model_counts"] });
  };

  const activeTools = (tools.data || []).filter((t) => t.gateway_active).length;

  // ---- 來源卡片（呈現抽到 providers/SourceCard.tsx；拖拽邏輯在 useDragSort） ----
  const srcCard = (p: Provider, i: number) => (
    <SourceCard
      key={p.id}
      p={p}
      i={i}
      selId={selId}
      dragId={dragId}
      modelCount={countMap.get(p.id) ?? 0}
      gripDown={gripDown}
      suppressClick={suppressClick}
      onSelect={setSelected}
      onEdit={(q) => {
        setSelected(q.id);
        setEditing(q);
      }}
      onDuplicate={(q) =>
        dup.mutate(q.id, {
          onSuccess: (newId) => {
            setSelected(newId);
            setListMsg(`已複製「${q.name}」`);
          },
          onError: (e) => setListMsg(`複製失敗：${String(e)}`),
        })
      }
      onDelete={(q) =>
        ask(`刪除來源 ${q.name}？`, () => del.mutate(q.id), {
          message: "該來源的使用模型與定價將一併移除，已分發的 Key 會失效。",
        })
      }
    />
  );

  const srcList = providers.data || [];

  return (
    <div className="space-y-4">
      <div className="glass p-5">
        <div className="mb-3">
          <Segmented
            className="w-full"
            btnClassName="px-3 py-3 text-sm font-medium"
            value={view}
            onChange={(v) => setView(v as "sources" | "tools")}
            options={[
              { id: "sources", label: `來源（${providers.data?.length ?? 0}）` },
              {
                id: "tools",
                label: `本機工具（${activeTools}/${tools.data?.length ?? 0} 接管中）`,
              },
            ]}
          />
        </div>
        {view === "sources" && (
          <div className="grid items-start gap-4 lg:grid-cols-2">
            <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <button
                className="btn-primary flex items-center gap-1.5 px-4 py-1.5 text-sm"
                onClick={() => setEditing("new")}
              >
                <Icon name="plus" size={13} />
                添加來源
              </button>
              <ProvidersIO count={providers.data?.length ?? 0} />
            </div>
            {listMsg && (
              <p className="mb-2 text-xs text-emerald-400/80">{listMsg}</p>
            )}
            {providers.isPending && (
              <p className="text-sm text-white/30">載入中…</p>
            )}
        {listErr && (
          <p className="mb-2 break-all text-xs text-red-400">{listErr}</p>
        )}
        <div ref={listRef} className="relative space-y-2">
          {srcList.map((p, i) => srcCard(p, i))}
          {dragId != null && drop != null && (
            <div
              key="drop-line"
              className="pointer-events-none absolute right-0 left-0 h-1 rounded-full bg-[#0A84FF]"
              style={{ top: drop.y }}
            />
          )}
        </div>
            </div>
            <div className="min-w-0 lg:sticky lg:top-0">
              <SourceDetail
                sel={sel}
                providers={providers.data || []}
                detailTab={detailTab}
                onDetailTab={setDetailTab}
              />
            </div>
          </div>
        )}
        {view === "sources" && (
          <div className="pt-4">
            <FailoverQueue />
          </div>
        )}
        {view === "tools" && (
          <>
        <ToolList
          tools={tools.data || []}
          isPending={tools.isPending}
          error={tools.isError ? tools.error : null}
          toggling={toggling}
          rowErr={rowErr}
          rowMsg={rowMsg}
          doctorLines={doctorLines}
          doctorBusy={doctorBusy}
          onOpen={setSwitching}
          onToggleRow={toggleRow}
          onRunDoctor={runDoctor}
        />
          </>
        )}
      </div>
      {switching && (
        <SwitchDialog
          tool={switching}
          providers={providers.data || []}
          port={port}
          onClose={() => setSwitching(null)}
          onApplied={refresh}
          initialProviderId={selId}
        />
      )}
      {editing && (
        <ProviderSheet
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            refresh();
          }}
        />
      )}
      <div className="glass px-5 py-4">
        <button
          onClick={() => setGpOpen((o) => !o)}
          className="flex w-full items-center gap-2 text-left"
        >
          <Icon
            name="chevron-right"
            size={13}
            className={`shrink-0 text-white/25 transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] ${gpOpen ? "rotate-90" : ""}`}
          />
          <span className="text-sm font-semibold tracking-tight text-white/80">
            {view === "tools" ? "本機工具版本" : "全域預設定價"}
          </span>
          {view !== "tools" && gpCount !== null && (
            <span className="rounded-full bg-white/[0.07] px-2 py-px text-[10px] font-medium text-white/45">
              {gpCount}
            </span>
          )}
          <span className="truncate text-xs text-white/30">
            {view === "tools"
              ? "已安裝工具的實際版本與一鍵更新"
              : "來源未單獨定價時的回退"}
          </span>
        </button>
        <div className="acc-body" data-open={gpOpen}>
          <div>
            <div className="mt-3 border-t border-white/[0.06] pt-3">
              {view === "tools" ? (
                <ToolVersions />
              ) : (
                <GlobalPricingManager onCount={setGpCount} />
              )}
            </div>
          </div>
        </div>
      </div>
      {confirmDialog}
    </div>
  );
}
