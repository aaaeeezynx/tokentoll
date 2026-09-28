//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

import type { ReactNode } from "react";
import { Icon } from "../icons";

export function SectionHead(props: {
  icon: "alert" | "bolt-fill" | "clock" | "sliders";
  tile: string;
  title: string;
  caption?: string;
  right?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-center gap-2.5">
      <span
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-white"
        style={{ background: props.tile }}
      >
        <Icon name={props.icon} size={17} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[15px] font-semibold tracking-tight text-white">
          {props.title}
        </div>
        {props.caption && (
          <div className="truncate text-[11px] text-white/30">
            {props.caption}
          </div>
        )}
      </div>
      {props.right}
    </div>
  );
}
