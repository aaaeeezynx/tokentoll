import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as RPointerEvent,
} from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  gatewayUrl,
  useGatewayPort,
  type Provider,
  type SwitchRequest,
  type ToolStatus,
} from "../lib/api";
import { Icon } from "./icons";
import { Segmented } from "./Segmented";
import { Toggle } from "./Toggle";
import { useConfirm } from "./Confirm";
import { Logo } from "./logos";
import GlobalPricingManager from "./Pricing";
import { ChannelPricing, ModelPanel } from "./providers/ModelCatalog";
import { ProviderSheet, ToolVersions } from "./providers/ProviderForm";
import { SwitchDialog } from "./providers/SwitchDialog";
import { loadLastSwitch, saveLastSwitch } from "./providers/providersTypes";

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
  const { dialog: confirmDialog, ask } = useConfirm();

  const [toggling, setToggling] = useState<string | null>(null);
  const [rowErr, setRowErr] = useState<{ app: string; msg: string } | null>(null);
  const [rowMsg, setRowMsg] = useState<{ app: string; msg: string } | null>(null);
  const [doctorLines, setDoctorLines] = useState<string[] | null>(null);
  const [doctorBusy, setDoctorBusy] = useState(false);
  // 來源拖拽排序（零 DOM 位移：開拖快照容器內座標，殘影 transform 跟手，
  // 藍線絕對定位；window 級監聽保證任何鬆手位置/手柄狀態都不丟事件）。
  const [dragId, setDragId] = useState<number | null>(null);
  const [drop, setDropState] = useState<{ idx: number; y: number } | null>(null);
  const [reordering, setReordering] = useState(false);
  const [listErr, setListErr] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{
    id: number;
    startY: number;
    lastY: number;
    moved: boolean;
    cardEl: HTMLElement | null;
  } | null>(null);
  const boundsRef = useRef<Array<{ top: number; bottom: number }> | null>(null);
  const idsRef = useRef<number[]>([]);
  const dropRef = useRef<{ idx: number; y: number } | null>(null);
  const suppressClick = useRef(false);
  const rafRef = useRef<number | null>(null);
  const detachWinRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const clearSuppress = () => {
      suppressClick.current = false;
    };
    window.addEventListener("pointerdown", clearSuppress, true);
    return () => {
      window.removeEventListener("pointerdown", clearSuppress, true);
      detachWinRef.current?.();
    };
  }, []);

  /** 提交全排列（拖拽落點 / 未來鍵盤排序共用）。 */
  const commitOrder = async (ids: number[]) => {
    setReordering(true);
    setListErr(null);
    try {
      await api.providerReorder(ids);
      void qc.invalidateQueries({ queryKey: ["providers"] });
    } catch (e) {
      setListErr(String(e));
      void qc.invalidateQueries({ queryKey: ["providers"] }); // 失敗回退到庫內真序
    } finally {
      setReordering(false);
    }
  };

  const setDrop = (v: { idx: number; y: number } | null) => {
    dropRef.current = v;
    setDropState(v);
  };

  /** 以開拖快照的容器內座標計算插入位與藍線 Y（拖拽期間版面零位移，快照全程有效）。 */
  const computeDrop = (): { idx: number; y: number } | null => {
    const d = dragRef.current;
    const box = listRef.current;
    const bounds = boundsRef.current;
    if (!d || !box || !bounds || bounds.length < 2) return null;
    const k = idsRef.current.indexOf(d.id);
    if (k < 0) return null;
    const y = d.lastY - box.getBoundingClientRect().top;
    let idx = 0;
    const rest: number[] = [];
    for (let i = 0; i < bounds.length; i++) {
      if (i === k) continue;
      rest.push(i);
      if (y > (bounds[i].top + bounds[i].bottom) / 2) idx += 1;
    }
    let lineY: number;
    if (idx === 0) lineY = bounds[rest[0]].top - 4;
    else if (idx >= rest.length) lineY = bounds[rest[rest.length - 1]].bottom + 4;
    else lineY = (bounds[rest[idx - 1]].bottom + bounds[rest[idx]].top) / 2;
    return { idx, y: lineY };
  };

  /** 拖拽唯一出口：拆 window 監聽、取消 rAF、清殘影 transform。 */
  const teardownDrag = () => {
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    detachWinRef.current?.();
    const d = dragRef.current;
    if (d?.cardEl) d.cardEl.style.transform = "";
    dragRef.current = null;
    boundsRef.current = null;
    idsRef.current = [];
  };

  /** rAF 節流：殘影跟手（imperative transform，不進 React）+ 落點僅在變化時 setState。 */
  const scheduleDragFrame = () => {
    if (rafRef.current != null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const d = dragRef.current;
      if (!d || !d.moved) return;
      if (d.cardEl) d.cardEl.style.transform = `translateY(${d.lastY - d.startY}px)`;
      const v = computeDrop();
      if (v && dropRef.current?.idx !== v.idx) setDrop(v);
    });
  };

  const onWinMove = (e: PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    d.lastY = e.clientY;
    if (!d.moved) {
      if (Math.abs(e.clientY - d.startY) <= 4) return;
      d.moved = true;
    }
    scheduleDragFrame();
  };

  const onWinCancel = () => {
    teardownDrag();
    setDragId(null);
    setDrop(null);
  };

  const onWinKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") onWinCancel();
  };

  const onWinUp = (e: PointerEvent) => {
    const d = dragRef.current;
    if (!d) {
      teardownDrag();
      return;
    }
    d.lastY = e.clientY;
    const final = d.moved ? computeDrop() : null;
    teardownDrag();
    setDragId(null);
    setDrop(null);
    if (!d.moved || !final) return;
    suppressClick.current = true;
    const list = providers.data || [];
    const rest = list.map((p) => p.id).filter((x) => x !== d.id);
    const clamped = Math.max(0, Math.min(final.idx, rest.length));
    rest.splice(clamped, 0, d.id);
    const orig = list.map((p) => p.id);
    if (rest.join(",") === orig.join(",")) return; // 原地，不提交
    void commitOrder(rest);
  };

  /** 手柄按下：快照容器內座標、掛 window 監聽，開始一次潛在拖拽（未移動則視為普通點擊）。 */
  const gripDown = (e: RPointerEvent<HTMLSpanElement>, id: number) => {
    if (reordering) return;
    e.preventDefault();
    const box = listRef.current;
    if (!box) return;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* 舊 webview 無 capture 也能跑，window 監聽兜底 */
    }
    const bounds: Array<{ top: number; bottom: number }> = [];
    const ids: number[] = [];
    for (const child of Array.from(box.children) as HTMLElement[]) {
      const pid = Number(child.dataset.pid);
      if (!pid) continue;
      const top = child.offsetTop;
      bounds.push({ top, bottom: top + child.offsetHeight });
      ids.push(pid);
    }
    boundsRef.current = bounds;
    idsRef.current = ids;
    dragRef.current = {
      id,
      startY: e.clientY,
      lastY: e.clientY,
      moved: false,
      cardEl: box.querySelector<HTMLElement>(`[data-pid="${id}"]`),
    };
    const detach = () => {
      window.removeEventListener("pointermove", onWinMove);
      window.removeEventListener("pointerup", onWinUp);
      window.removeEventListener("pointercancel", onWinCancel);
      window.removeEventListener("keydown", onWinKey);
      detachWinRef.current = null;
    };
    detachWinRef.current = detach;
    window.addEventListener("pointermove", onWinMove);
    window.addEventListener("pointerup", onWinUp);
    window.addEventListener("pointercancel", onWinCancel);
    window.addEventListener("keydown", onWinKey);
    setDragId(id);
    setDrop(null);
    setListErr(null);
  };

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

  /** 行開關（只開/關，永不彈框）：關=還原接管前備份；開=按上次來源/模型強制經網關接管。詳情設定一律點行左側進入。 */
  const toggleRow = (t: ToolStatus, on: boolean) => {
    setRowErr(null);
    setRowMsg(null);
    if (!on) {
      setToggling(t.app);
      api
        .switchRestore(t.app, port)
        .then((msg) => {
          setRowMsg({ app: t.app, msg });
          refreshTools();
        })
        .catch((e) => setRowErr({ app: t.app, msg: String(e) }))
        .finally(() => setToggling(null));
      return;
    }
    void toggleOn(t);
  };

  /** 開關 ON：上次參數可用則沿用來源/模型/Key 偏好，但路由強制為本網關。 */
  const toggleOn = async (t: ToolStatus) => {
    const saved = loadLastSwitch(t.app);
    const savedKey = saved?.api_key ?? "";
    if (saved) {
      const req: SwitchRequest = {
        ...saved,
        app: t.app,
        base_url: gatewayUrl(port, t.app),
        api_key: savedKey,
        gen_catalog: saved.gen_catalog ?? true,
        catalog_union: saved.catalog_union ?? false,
        direct_upstream: !savedKey.trim(),
      };
      await doApply(t, req, "已接管", req.direct_upstream);
      return;
    }
    const provider = (providers.data || []).find((p) => p.id === selId);
    if (!provider) {
      setRowErr({ app: t.app, msg: "請先選擇一個上游來源" });
      return;
    }
    try {
      const { req, upstreamKeyWritten } = await resolveSelectionReq(t, provider);
      await doApply(t, req, "已接管", upstreamKeyWritten);
    } catch (e) {
      setRowErr({ app: t.app, msg: e instanceof Error ? e.message : String(e) });
    }
  };

  /**
   * 按指定來源組裝接管請求（開關無參數回退 / 來源切換自動重接管共用，永不彈框）。
   * 模型：上次/現行仍在該來源啟用表則沿用，否則首個啟用模型；
   * Key：所有工具一致可空（留空 = 以該來源上游 Key 直連，後端注入；用量照常記錄）。
   */
  const resolveSelectionReq = async (
    t: ToolStatus,
    provider: Provider,
  ): Promise<{ req: SwitchRequest; upstreamKeyWritten: boolean }> => {
    const saved = loadLastSwitch(t.app);
    const apiKey = saved?.api_key ?? "";
    const upstreamKeyWritten = !apiKey.trim();
    if (upstreamKeyWritten && !(provider.api_key ?? "").trim()) {
      throw new Error(`來源 ${provider.name} 未填寫上游 Key，無法直連`);
    }
    const models = await api.modelsList(provider.id);
    const enabled = models.filter((m) => m.enabled);
    if (enabled.length === 0) {
      throw new Error(`來源 ${provider.name} 沒有啟用的模型`);
    }
    const currentModel =
      (tools.data || []).find((x) => x.app === t.app)?.current_model ?? null;
    const candidates = [saved?.model, currentModel].filter(Boolean) as string[];
    const hit = enabled.find((m) => candidates.includes(m.actual_model)) ?? null;
    const row = hit ?? enabled[0];
    const reasoning =
      row.actual_model === saved?.model && saved?.reasoning
        ? saved.reasoning
        : row.reasoning !== "unset"
          ? (row.reasoning.split(",")[0]?.trim() || null)
          : null;
    return {
      req: {
        app: t.app,
        base_url: gatewayUrl(port, t.app),
        api_key: apiKey,
        model: row.actual_model,
        provider_id: provider.id,
        provider_format: provider.api_format ?? null,
        reasoning,
        context_window: row.context_window ?? null,
        gen_catalog: (saved?.gen_catalog ?? true) && t.app === "codex",
        catalog_union: saved?.catalog_union ?? false,
        direct_upstream: !apiKey.trim(),
        key_id: saved?.key_id ?? null,
      },
      upstreamKeyWritten,
    };
  };

  /** 執行接管寫入（開關/自動共用）：存檔＋行內成功信息＋刷新；失敗只報行內錯誤。 */
  const doApply = async (
    t: ToolStatus,
    req: SwitchRequest,
    okPrefix: string,
    upstreamKeyWritten: boolean,
  ) => {
    setToggling(t.app);
    try {
      await api.switchApply(req, port);
      saveLastSwitch(t.app, req);
      const pname =
        (providers.data || []).find((p) => p.id === req.provider_id)?.name ?? "";
      const keyNote = upstreamKeyWritten
        ? "（直連上游 Key 已寫入配置文件，僅本機可讀）"
        : "";
      setRowMsg({
        app: t.app,
        msg: `${okPrefix}：${req.model}${pname ? ` @ ${pname}` : ""}${keyNote}`,
      });
      refreshTools();
    } catch (e) {
      setRowErr({ app: t.app, msg: String(e) });
    } finally {
      setToggling(null);
    }
  };

  /**
   * 來源切換自動重接管共用入口（按當前所選來源組裝，永不彈框）。
   * 模型解析：上次模型（或現行模型）仍在該來源啟用表則沿用，否則取該來源首個啟用模型；
   * Key/目錄/推理偏好沿用上次設定。來源無啟用模型時報行內錯誤，不寫入。
   */
  const applyCurrentSelection = async (t: ToolStatus) => {
    const provider = (providers.data || []).find((p) => p.id === selId);
    if (!provider) {
      setRowErr({ app: t.app, msg: "請先選擇一個上游來源" });
      return;
    }
    try {
      const { req, upstreamKeyWritten } = await resolveSelectionReq(t, provider);
      await doApply(t, req, "已按新來源重接管", upstreamKeyWritten);
    } catch (e) {
      setRowErr({ app: t.app, msg: e instanceof Error ? e.message : String(e) });
    }
  };

  // 來源切換自動重接管：所選來源變化且 Codex 正被接管時，按新來源直接重接管
  //（模型沿用現行/上次，否則新來源首個啟用模型；Codex 重啟後選擇器同步）。
  const prevSelId = useRef<number | null>(null);
  useEffect(() => {
    if (prevSelId.current === null) {
      prevSelId.current = selId;
      return;
    }
    if (prevSelId.current === selId) return;
    prevSelId.current = selId;
    if (selId == null) return;
    const codex = (tools.data || []).find((x) => x.app === "codex");
    if (!codex?.gateway_active || toggling === "codex") return;
    void applyCurrentSelection(codex);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selId, tools.data, toggling]);

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ["providers"] });
    void qc.invalidateQueries({ queryKey: ["tools", port] });
    void qc.invalidateQueries({ queryKey: ["db_status"] });
    void qc.invalidateQueries({ queryKey: ["model_counts"] });
  };

  const activeTools = (tools.data || []).filter((t) => t.gateway_active).length;

  // ---- 來源卡片（指針拖拽：手柄 pointerdown/move/up，接管 pointer capture） ----
  const srcCard = (p: Provider, i: number) => (
    <div
      key={p.id}
      data-pid={p.id}
      onClick={() => {
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        setSelected(p.id);
      }}
      title="點擊選中，在下方查看模型與定價；按住左側手柄拖拽排序"
      style={{
        animationDelay: `${Math.min(i * 50, 300)}ms`,
        ...(dragId === p.id
          ? { position: "relative", zIndex: 10, transition: "none" }
          : {}),
      }}
      data-active={p.id === selId}
      className={`sel-card anim-rise cursor-pointer rounded-xl border p-3.5 text-sm transition-all ${
        p.id === selId
          ? ""
          : "border-white/[0.06] hover:border-white/10"
      } ${dragId === p.id ? "opacity-40" : ""}`}
    >
      <div className="flex items-center gap-2">
        <span
          title="按住拖拽排序"
          onPointerDown={(e) => gripDown(e, p.id)}
          style={{ touchAction: "none" }}
          className="grid shrink-0 cursor-grab grid-cols-2 gap-x-[3px] gap-y-[3px] px-1 text-white/20 select-none active:cursor-grabbing"
        >
          {[0, 1, 2, 3, 4, 5].map((d) => (
            <span key={d} className="h-[3px] w-[3px] rounded-full bg-current" />
          ))}
        </span>
        <span className="min-w-0 flex-1 truncate font-semibold text-white/85">
          {p.name}
          <span className="ml-2 rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-medium text-white/30">
            {countMap.get(p.id) ?? 0} 模型
          </span>
        </span>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-white/10 hover:text-white"
          title="編輯"
          onClick={(e) => {
            e.stopPropagation();
            setSelected(p.id);
            setEditing(p);
          }}
        >
          <Icon name="pencil" size={13} />
        </button>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="刪除"
          onClick={(e) => {
            e.stopPropagation();
            ask(`刪除來源 ${p.name}？`, () => del.mutate(p.id), {
              message: "該來源的使用模型與定價將一併移除，已分發的 Key 會失效。",
            });
          }}
        >
          <Icon name="trash" size={13} />
        </button>
      </div>
      <p className="mt-1.5 truncate font-mono text-xs text-white/30">
        {p.base_url}
      </p>
      <div className="mt-0.5 flex items-center gap-2 text-xs">
        <p className="min-w-0 flex-1 truncate text-white/20">
          {(p.models || []).join(", ") || "未登記模型"}
        </p>
        <span className="shrink-0 text-[10px] text-white/25">
          {p.app_type} · {p.api_format} ·{" "}
          {p.auth_scheme === "goog-key" ? "Google Key" : "Bearer"}
        </span>
      </div>
    </div>
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
            <button
              className="btn-primary mb-3 flex items-center gap-1.5 px-4 py-1.5 text-sm"
              onClick={() => setEditing("new")}
            >
              <Icon name="plus" size={13} />
              添加來源
            </button>
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
              {sel && (
                <div key={sel.id} className="glass anim-rise p-5">
                  <div className="mb-4 flex flex-wrap items-center gap-2.5">
                    <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/70">
                      <Icon name="server" size={18} />
                    </span>
                    <div className="min-w-0 flex-1 basis-40">
                      <div className="truncate text-[15px] font-semibold tracking-tight text-white">
                        {sel.name}
                      </div>
                      <div className="truncate font-mono text-[11px] text-white/30">
                        {sel.base_url}
                      </div>
                    </div>
                    <div className="hidden flex-1 sm:block" />
                    <Segmented
                      value={detailTab}
                      onChange={(v) => setDetailTab(v as "models" | "pricing")}
                      options={[
                        { id: "models", label: "模型" },
                        { id: "pricing", label: "定價" },
                      ]}
                      btnClassName="px-3.5 py-1 text-[13px] font-medium"
                    />
                  </div>
                  {detailTab === "models" ? (
                    <>
                      <div className="mb-2 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                        <span>使用模型</span>
                      </div>
                      <ModelPanel key={sel.id} provider={sel} />
                    </>
                  ) : (
                    <ChannelPricing
                      key={`pp-${sel.id}`}
                      providers={providers.data || []}
                      focusPid={sel.id}
                    />
                  )}
                </div>
              )}
            </div>
          </div>
        )}
        {view === "tools" && (
          <>
        {tools.isPending && <p className="text-sm text-white/30">檢測中…</p>}
        {tools.isError && (
          <p className="text-sm text-red-400">檢測失敗：{String(tools.error)}</p>
        )}
        <div className="space-y-2">
          {(tools.data || []).map((t, i) => (
            <div
              key={t.app}
              style={{ animationDelay: `${Math.min(i * 50, 300)}ms` }}
              onClick={() => setSwitching(t)}
              title="點擊進入切換設定"
              className="glass-float hover-line anim-rise cursor-pointer rounded-xl border p-3.5 text-sm"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-2 font-semibold text-white/85">
                  <Logo name={t.app} size={17} className="shrink-0" />
                  <span className="truncate">{t.display}</span>
                  {!t.installed && (
                    <span className="shrink-0 text-xs text-white/20">未安裝</span>
                  )}
                  {t.gateway_active && (
                    <span className="shrink-0 rounded-full bg-[#30d158]/10 px-2.5 py-0.5 text-[10px] font-medium text-[#30d158]">
                      網關接管中
                    </span>
                  )}
                </span>
                <Toggle
                  size="sm"
                  checked={t.gateway_active}
                  disabled={!t.supported_switch || toggling === t.app}
                  title={
                    t.supported_switch
                      ? t.gateway_active
                        ? "關閉：還原接管前備份"
                        : "開啟：直接接管（詳情點行左側）"
                      : "僅檢測，自動切換待支援"
                  }
                  onChange={(v) => toggleRow(t, v)}
                />
              </div>
              {t.gateway_active && (
                <>
                  <p className="mt-1.5 truncate font-mono text-xs text-white/30">
                    {t.current_base_url || "（未檢測到 base_url）"}
                  </p>
                  <p className="mt-0.5 truncate text-xs text-white/20">
                    模型：{t.current_model || "未知"}
                  </p>
                </>
              )}
              {rowErr?.app === t.app && (
                <p className="mt-1.5 break-all text-xs text-red-400">{rowErr.msg}</p>
              )}
              {rowMsg?.app === t.app && (
                <p className="mt-1.5 break-all text-xs text-white/45">{rowMsg.msg}</p>
              )}
              {t.app === "codex" && (
                <div className="mt-1.5">
                  <button
                    className="rounded-full border border-white/10 px-2.5 py-0.5 text-[11px] text-white/50 transition-colors hover:border-white/20 hover:text-white"
                    disabled={doctorBusy}
                    onClick={() => runDoctor()}
                  >
                    {doctorBusy ? "體檢中…" : "接管前體檢"}
                  </button>
                  {doctorLines && (
                    <ul className="mt-1.5 space-y-0.5 font-mono text-[11px] leading-relaxed">
                      {doctorLines.map((l, i) => (
                        <li
                          key={i}
                          className={
                            l.startsWith("❌")
                              ? "text-red-400"
                              : l.startsWith("⚠️")
                                ? "text-amber-300/90"
                                : l.startsWith("ℹ️")
                                  ? "text-white/50"
                                  : "text-[#30d158]/80"
                          }
                        >
                          {l}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
        <p className="mt-3 text-xs text-white/15">
          DSH 僅檢測不寫入設定（endpoint 由 profile bundle 決定）；請手動把
          base_url 指向本網關，並在請求頭帶 x-tg-app 標識。
          Cursor／Antigravity 不提供接管，只作為用量歸屬標籤出現在用量頁。
        </p>
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
