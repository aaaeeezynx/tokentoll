//! 由 `components/providers/SwitchDialog.tsx` 拆分而來（原檔 574 行）。程式碼語意未改，只搬位置。

import type {
  Provider,
  PricingMode,
  ToolStatus,
} from "../../../lib/api";

export type SwitchDialogProps = {
  tool: ToolStatus;
  providers: Provider[];
  port: number;
  onClose: () => void;
  onApplied: () => void;
    /** 呼叫時來源列表的選中項：對話框預設跟隨它，而非固定取第一個來源 */
  initialProviderId: number | null;
};

export const MODE_LABEL: Record<PricingMode, string> = {
  usage: "按量",
  subscription: "訂閱",
  tou: "峰谷",
};
