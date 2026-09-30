import type { Provider } from "../../lib/api";
import { Icon } from "../icons";
import { Segmented } from "../Segmented";
import { ConnectivityCheck } from "./ConnectivityCheck";
import { ChannelPricing, ModelPanel } from "./ModelCatalog";

/**
 * 右側「選中來源」詳情面板：標題、模型／定價分頁、以及對應的內容。
 *
 * 由 `Providers.tsx` 抽出（該檔 2026-09-28 時 465 行）。純呈現：
 * 選中的來源與分頁狀態由呼叫端持有。JSX 與文案一字未改。
 */
export function SourceDetail(props: {
  /** 目前選取的來源；null 時整個面板不渲染。 */
  sel: Provider | null;
  /** 所有來源（定價分頁需要完整清單）。 */
  providers: Provider[];
  /** 目前的分頁。 */
  detailTab: "models" | "pricing";
  onDetailTab: (v: "models" | "pricing") => void;
}) {
  const { sel } = props;
  if (!sel) return null;
  return (
    <div key={sel.id} className="glass anim-rise p-5">
      <div className="mb-4 flex flex-wrap items-center gap-2.5">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/70">
          <Icon name="server" size={18} />
        </span>
        <div className="min-w-0 flex-1 basis-40">
          <div className="truncate text-[15px] font-semibold tracking-tight text-fg">
            {sel.name}
          </div>
          <div className="truncate font-mono text-[11px] text-fg/30">
            {sel.base_url}
          </div>
        </div>
        <div className="hidden flex-1 sm:block" />
        <Segmented
          value={props.detailTab}
          onChange={(v) => props.onDetailTab(v as "models" | "pricing")}
          options={[
            { id: "models", label: "模型" },
            { id: "pricing", label: "定價" },
          ]}
          btnClassName="px-3.5 py-1 text-[13px] font-medium"
        />
      </div>
      {props.detailTab === "models" ? (
        <>
          <div className="mb-2 text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
            <span>使用模型</span>
          </div>
          <ModelPanel key={sel.id} provider={sel} />
          {/* 連線檢查（P1.5）：只測位址可達性，不送模型請求 */}
          <ConnectivityCheck
            key={`chk-${sel.id}`}
            baseUrl={sel.base_url}
          />
        </>
      ) : (
        <ChannelPricing
          key={`pp-${sel.id}`}
          providers={props.providers}
          focusPid={sel.id}
        />
      )}
    </div>
  );
}
