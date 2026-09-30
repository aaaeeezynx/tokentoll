import type { PointerEvent as RPointerEvent, ReactNode, RefObject } from "react";
import type { Provider } from "../../lib/api";
import { Icon } from "../icons";

/**
 * 「上游來源」清單中的一張卡片。
 *
 * 由 `Providers.tsx` 抽出（該檔 2026-09-28 時 613 行）。純呈現元件：
 * 所有狀態與行為都由 props 帶進來，這裡不碰任何 hook 或 API。
 * JSX 與文案一字未改。
 */
export function SourceCard(props: {
  p: Provider;
  /** 在清單中的索引（只用於進場動畫的延遲）。 */
  i: number;
  /** 目前選取的來源 id。 */
  selId: number | null;
  /** 正在被拖拽的來源 id。 */
  dragId: number | null;
  /** 該來源的模型數量。 */
  modelCount: number;
  /** 拖拽手柄的 pointerdown。 */
  gripDown: (e: RPointerEvent<HTMLSpanElement>, id: number) => void;
  /** 「這次點擊是拖拽收尾」旗標；為 true 時吞掉該次點擊。 */
  suppressClick: RefObject<boolean>;
  onSelect: (id: number) => void;
  onEdit: (p: Provider) => void;
  onDuplicate: (p: Provider) => void;
  onDelete: (p: Provider) => void;
  /** 卡片底部的用量／餘額顯示（P2.2；沒設定時是一個入口按鈕）。 */
  quotaSlot?: ReactNode;
}) {
  const { p, i, selId, dragId, modelCount, gripDown, suppressClick } = props;
  return (
    <div
      key={p.id}
      data-pid={p.id}
      onClick={() => {
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        props.onSelect(p.id);
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
            {modelCount} 模型
          </span>
        </span>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-white/10 hover:text-white"
          title="編輯"
          onClick={(e) => {
            e.stopPropagation();
            props.onEdit(p);
          }}
        >
          <Icon name="pencil" size={13} />
        </button>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-white/10 hover:text-white"
          title="複製來源（連模型、定價、訂閱時段一起，金鑰綁定不會複製）"
          onClick={(e) => {
            e.stopPropagation();
            props.onDuplicate(p);
          }}
        >
          <Icon name="copy" size={13} />
        </button>
        <button
          className="shrink-0 rounded-full p-1.5 text-white/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
          title="刪除"
          onClick={(e) => {
            e.stopPropagation();
            props.onDelete(p);
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
      {props.quotaSlot}
    </div>
  );
}
