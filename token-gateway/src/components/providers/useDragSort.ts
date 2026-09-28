import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from "react";

/**
 * 來源清單的拖拽排序。
 *
 * 從 `Providers.tsx` 抽出來的（該檔原本 778 行，這一塊佔約 190 行）。
 * 抽出的界線是「與 React 無關的指標運算」：本 hook 只負責
 * 「算出新的排列 → 交給 `onCommit`」，實際送 API 與 invalidate 由呼叫端決定。
 *
 * 設計要點（原註解保留）：
 * - **零 DOM 位移**：開拖時快照容器內座標，殘影用 transform 跟手，
 *   藍線絕對定位；拖拽期間版面完全重排，所以快照全程有效。
 * - **window 級監聽**：保證任何鬆手位置／手柄狀態都不丟事件。
 * - **未移動即點擊**：移動距離 ≤ 4px 視為普通點擊，不觸發排序。
 */
export type DragSort = {
  /** 目前被拖的項目 id（給卡片上樣式用）。 */
  dragId: number | null;
  /** 藍線位置；`y` 是容器內座標。 */
  drop: { idx: number; y: number } | null;
  /** 是否正在提交新順序。 */
  reordering: boolean;
  /** 提交失敗的訊息。 */
  listErr: string | null;
  /** 設定／清除錯誤訊息（呼叫端在切換檢視等時機可自行清）。 */
  setListErr: (v: string | null) => void;
  /** 清單容器 ref，掛在最外層 div 上。 */
  listRef: React.RefObject<HTMLDivElement | null>;
  /** 手柄的 onPointerDown。 */
  gripDown: (e: RPointerEvent<HTMLSpanElement>, id: number) => void;
  /**
   * 「這次點擊是拖拽的收尾，不要當成選取」旗標。
   * 卡片 `onClick` 要先讀它；`true` 時吞掉該次點擊並設回 `false`。
   */
  suppressClick: React.RefObject<boolean>;
};

export function useDragSort(
  ids: number[],
  onCommit: (ids: number[]) => Promise<void>,
): DragSort {
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
  // 最新順序：window 監聽是一次性掛上的閉包，直接讀 prop 會拿到舊值。
  const idsLatest = useRef<number[]>(ids);
  idsLatest.current = ids;

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
    const list = idsLatest.current;
    const rest = list.filter((x) => x !== d.id);
    const clamped = Math.max(0, Math.min(final.idx, rest.length));
    rest.splice(clamped, 0, d.id);
    if (rest.join(",") === list.join(",")) return; // 原地，不提交
    setReordering(true);
    setListErr(null);
    void onCommit(rest).finally(() => setReordering(false));
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
    const snapIds: number[] = [];
    for (const child of Array.from(box.children) as HTMLElement[]) {
      const pid = Number(child.dataset.pid);
      if (!pid) continue;
      const top = child.offsetTop;
      bounds.push({ top, bottom: top + child.offsetHeight });
      snapIds.push(pid);
    }
    boundsRef.current = bounds;
    idsRef.current = snapIds;
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

  return {
    dragId,
    drop,
    reordering,
    listErr,
    setListErr,
    listRef,
    gripDown,
    suppressClick,
  };
}
