/**
 * 用量查詢面板的兩個獨立區塊（從 `UsageQueryPanel.tsx` 拆出來）。
 *
 * 原因：那支檔案長到 411 行，超過專案的「每個檔案 ≤400 行」上限。
 * 這兩個區塊本來就是自成一格的視覺單位（規格輸入／測試結果），
 * 搬出來之後面板只留欄位與流程。
 */
import type { QuotaView, UsageQueryConfig } from "../../lib/api";

/** 抽取規格 ＋ 進階（逾時、自動查詢間隔）。 */
export function UsageQuerySpecAdvanced(props: {
  cfg: UsageQueryConfig;
  spec: string;
  specErr: string;
  fieldCls: string;
  setSpec: (v: string) => void;
  set: <K extends keyof UsageQueryConfig>(k: K, v: UsageQueryConfig[K]) => void;
}) {
  const { cfg, spec, specErr, fieldCls, setSpec, set } = props;
  return (
    <>
      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
          抽取規格（怎麼從回應裡取數字）
        </div>
        <textarea
          className="field h-32 w-full px-2.5 py-2 font-mono text-[11px]"
          value={spec}
          onChange={(e) => setSpec(e.target.value)}
        />
        {specErr && <p className="text-[11px] text-red-400">{specErr}</p>}
        <p className="text-[11px] leading-relaxed text-fg/25">
          路徑用點分隔、數字段是陣列索引（例如
          <span className="font-mono"> balance_infos.0.total_balance</span>）；
          要換算就寫成物件（例如
          <span className="font-mono">{`{"path":"quota","divide":500000}`}</span>）。
          多方案用 <span className="font-mono">plans</span>（見 New API 樣板）。
        </p>
      </div>

      <div className="form-section">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
          進階
        </div>
        <div className="mac-frow">
          <span className="mac-cap">逾時（秒）</span>
          <input
            className={fieldCls}
            type="number"
            min={1}
            max={120}
            value={cfg.timeout_secs}
            onChange={(e) => set("timeout_secs", Number(e.target.value) || 10)}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">自動查詢（分）</span>
          <input
            className={fieldCls}
            type="number"
            min={0}
            max={1440}
            value={cfg.auto_interval_min}
            onChange={(e) => set("auto_interval_min", Number(e.target.value) || 0)}
          />
        </div>
        <p className="text-[11px] leading-relaxed text-fg/25">
          自動查詢 <span className="font-mono">0</span> ＝ 不自動查（預設）。
          cc-switch 只對「目前使用中」的來源自動查；我們也只在這個來源被選取時
          才按間隔重查，避免打擾上游。
        </p>
      </div>
    </>
  );
}

/** 測試結果（成功／失敗、餘額、原始回應）。 */
export function UsageQueryTestResult(props: {
  test: QuotaView;
  showRaw: boolean;
  onToggleRaw: () => void;
}) {
  const { test, showRaw, onToggleRaw } = props;
  return (
    <div className="rounded-lg bg-fg/[0.04] px-3 py-2.5 text-[12px]">
      <div className="flex items-center gap-2">
        <span
          className="rounded px-1.5 py-px text-[11px]"
          style={
            test.ok
              ? { background: "rgba(48,209,88,0.14)", color: "#30d158" }
              : { background: "rgba(255,69,58,0.16)", color: "#ff6961" }
          }
        >
          {test.ok ? "成功" : "失敗"}
        </span>
        <span className="min-w-0 flex-1 truncate text-fg/70">
          {test.ok
            ? `${test.plan_name || "餘額"}：${test.remaining ?? "—"} ${test.unit}${
                test.percent !== null ? `（用量 ${Math.round(test.percent)}%）` : ""
              }`
            : test.message}
        </span>
        <button
          className="btn-ghost shrink-0 px-2 py-0.5 text-[11px]"
          onClick={onToggleRaw}
        >
          {showRaw ? "收起回應" : "看回應"}
        </button>
      </div>
      {showRaw && (
        <pre className="mt-1.5 max-h-40 overflow-auto font-mono text-[10px] break-all whitespace-pre-wrap text-fg/45">
          {test.raw || "（空）"}
        </pre>
      )}
    </div>
  );
}