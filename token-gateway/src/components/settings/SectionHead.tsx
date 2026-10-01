/**
 * 設定頁的區塊標題（從 `Settings.tsx` 拆出來共用）。
 */
import { Icon, type IconName } from "../icons";

export function SectionHead(props: {
  icon: IconName;
  tile: string;
  title: string;
  caption: string;
}) {
  return (
    <div className="mb-3 flex items-center gap-3">
      <div
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-fg"
        style={{ background: props.tile }}
      >
        <Icon name={props.icon} size={17} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[15px] font-semibold tracking-tight text-fg">{props.title}</div>
        <div className="truncate text-[11px] text-fg/35">{props.caption}</div>
      </div>
    </div>
  );
}
