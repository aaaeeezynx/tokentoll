//! 由 `components/providers/SwitchDialog.tsx` 拆分而來（原檔 574 行）。程式碼語意未改，只搬位置。
//! 狀態與流程見 `./useSwitchDialog`。

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
  Logo,
} from "../../logos";
import {
  CLAUDE_TIERS,
  ClaudeTierField,
} from "../ProviderForm";
import {
  copyText,
} from "../providersTypes";
import {
  useSwitchDialog,
} from "./useSwitchDialog";
import type {
  SwitchDialogProps,
} from "./shared";

export function SwitchDialog(props: SwitchDialogProps) {
  const {
    closing,
    requestClose,
    tool,
    viaGateway,
    flip,
    switchDisabled,
    providers,
    providerId,
    setProviderId,
    enabledRows,
    modelRows,
    rowId,
    setRowId,
    inputCls,
    manualModel,
    setManualModel,
    row,
    claudeRaws,
    setClaudeRaws,
    effModel,
    keyId,
    pickKey,
    enabledLocalKeys,
    keyMsg,
    pickedKey,
    genCatalog,
    setGenCatalog,
    catalogUnion,
    setCatalogUnion,
    req,
    apply,
    plan,
    applySelection,
    result,
    confirmDialog,
  } = useSwitchDialog(props);

  return (
    <div
      className={`fixed inset-0 z-[80] flex items-center justify-center glass-backdrop p-4 transition-opacity duration-200 ${closing ? "opacity-0" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div
        className={`max-h-[90vh] w-full max-w-md overflow-y-auto overflow-x-clip glass-panel p-5 ${closing ? "sheet-out" : "pop-in"}`}
      >
        <div className="mb-4 flex items-center gap-2">
          <button
            className="rounded-full bg-white/[0.055] p-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
            onClick={requestClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <Logo name={tool.app} size={22} className="shrink-0" />
          <div className="flex-1 text-[15px] font-semibold tracking-tight text-white">
            切換 {tool.display}
          </div>
          <Toggle
            checked={viaGateway}
            onChange={flip}
            disabled={switchDisabled}
            title="經本地網關：開=寫入網關接管，關=還原接管前配置並切回原生來源"
          />
        </div>
        {providers.length === 0 ? (
          <p className="text-sm text-white/40">
            還沒有來源，請先在左側添加一個上游來源。
          </p>
        ) : (
          <div className="space-y-2.5">
            <PopSelect
              label="來源"
              value={providerId != null ? String(providerId) : ""}
              onChange={(v) => setProviderId(Number(v))}
              options={providers.map((p) => ({
                value: String(p.id),
                label: p.name,
                hint: p.base_url,
              }))}
            />
            {enabledRows.length > 0 ? (
              <>
                <PopSelect
                  label="模型"
                  searchable
                  value={String(rowId)}
                  onChange={(v) => setRowId(Number(v))}
                  options={[
                    ...modelRows.map((m) => ({
                      value: String(m.id),
                      label: m.display_name,
                      hint:
                        m.display_name !== m.actual_model
                          ? m.actual_model
                          : undefined,
                    })),
                    { value: "0", label: "手動輸入…" },
                  ]}
                />
                {rowId === 0 && (
                  <input
                    className={inputCls}
                    placeholder="實際請求模型 ID"
                    value={manualModel}
                    onChange={(e) => setManualModel(e.target.value)}
                  />
                )}
                {(row || manualModel.trim()) && (
                  <p className="font-mono text-xs text-white/40">
                    → {row ? row.actual_model : manualModel.trim()}
                    {row?.context_window
                      ? ` · ${row.context_window.toLocaleString()} ctx`
                      : ""}
                  </p>
                )}
              </>
            ) : (
              <input
                className={inputCls}
                placeholder="模型 ID（該來源尚無可用模型，請先到詳情頁新增）"
                value={manualModel}
                onChange={(e) => setManualModel(e.target.value)}
              />
            )}
            {tool.app === "claude" && (
              <div className="space-y-2.5 rounded-xl border border-white/[0.07] p-3">
                <p className="text-[12px] font-semibold text-white/60">
                  模型映射
                </p>
                <p className="-mt-1.5 text-[11px] text-white/30">
                  留空檔位跟隨上方選擇的模型，不寫入；其他工具不受影響。
                </p>
                {CLAUDE_TIERS.map((t) => (
                  <ClaudeTierField
                    key={t.key}
                    label={t.label}
                    options={modelRows.map((m) => ({
                      value: m.actual_model,
                      label: m.display_name,
                      hint:
                        m.display_name !== m.actual_model
                          ? m.actual_model
                          : undefined,
                    }))}
                    raw={claudeRaws[t.key]}
                    followedBy={effModel}
                    onRaw={(v) =>
                      setClaudeRaws((p) => ({ ...p, [t.key]: v }))
                    }
                  />
                ))}
              </div>
            )}
            <div>
              <PopSelect
                label="本地 Key"
                value={keyId}
                onChange={pickKey}
                options={[
                  {
                    value: "",
                    label: "不填：使用所選來源的上游 Key",
                    hint: "用量照常記錄",
                  },
                  ...enabledLocalKeys.map((k) => ({
                    value: String(k.id),
                    label: `${k.name}（${k.key_prefix}）`,
                    hint:
                      (k.provider_name ? `來源：${k.provider_name}` : "任意來源") +
                      (k.rate_limit_qpm > 0 ? ` · 限流 ${k.rate_limit_qpm}/分` : ""),
                  })),
                ]}
              />
              {keyMsg && <p className="mt-1 text-xs text-amber-400">{keyMsg}</p>}
              <p className="mt-1 text-[11px] text-white/30">
                {pickedKey
                    ? "經網關時以本地 Key 計量與限流；開關關閉則還原接管前配置並切回原生來源。"
                  : "留空 = 使用所選來源的上游 Key（不經本地 Key 轉發，用量照常記錄）"}
              </p>
            </div>
            {tool.app === "codex" && (
              <label className="flex items-center gap-2 text-[13px] text-white/60">
                <input
                  type="checkbox"
                  checked={genCatalog}
                  onChange={(e) => setGenCatalog(e.target.checked)}
                />
                同時產生 Codex 模型目錄
              </label>
            )}
            {tool.app === "codex" && genCatalog && (
              <label className="flex items-center gap-2 text-[13px] text-white/60">
                <input
                  type="checkbox"
                  checked={catalogUnion}
                  onChange={(e) => setCatalogUnion(e.target.checked)}
                />
                目錄包含其他來源模型（預設僅當前來源）
              </label>
            )}
            <button
              className="btn-primary w-full px-4 py-2 text-sm disabled:cursor-not-allowed disabled:opacity-40"
              disabled={!req || apply.isPending || plan.data?.supported === false}
              title="直接套用當前選擇的來源與模型，無需關閉再重開開關"
              onClick={applySelection}
            >
              {apply.isPending ? "套用中…" : "套用（切換來源/模型）"}
            </button>
            <div className="border-t border-white/[0.07]" />
            <div className="text-xs">
              {plan.isPending && <p className="text-white/30">正在生成方案…</p>}
              {plan.isError && (
                <p className="text-red-400">{String(plan.error)}</p>
              )}
              {plan.data && (
                <div className="space-y-2">
                  <p className="truncate font-mono text-white/40" title={plan.data.config_path}>
                    {plan.data.config_path}
                    <span className="ml-1.5 text-white/25">
                      {plan.data.will_backup ? "· 自動備份" : "· 將新建"}
                    </span>
                  </p>
                  {plan.data.edits.length > 0 && (
                    <details>
                      <summary className="cursor-pointer text-white/50">
                        {plan.data.edits.length} 項寫入
                      </summary>
                      <div className="mt-1.5 space-y-1">
                        {plan.data.edits.map((e) => (
                          <p key={e} className="text-white/45">
                            • {e}
                          </p>
                        ))}
                      </div>
                    </details>
                  )}
                  {plan.data.env_setup.map((e) => (
                    <div key={e} className="flex items-start gap-2">
                      <p className="flex-1 break-all font-mono text-amber-300">{e}</p>
                      <button
                        className="btn-ghost shrink-0 rounded-md px-2 py-0.5 text-xs"
                        onClick={() => copyText(e)}
                      >
                        複製
                      </button>
                    </div>
                  ))}
                  {plan.data.warnings.map((w) => (
                    <p key={w} className="flex items-start gap-1.5 text-amber-400">
                      <Icon name="alert" size={13} className="mt-0.5 shrink-0" />
                      {w}
                    </p>
                  ))}
                </div>
              )}
            </div>
            {result && (
              <p className="break-all whitespace-pre-wrap text-xs text-[#30d158]">
                {result}
              </p>
            )}
          </div>
        )}
      </div>
      {confirmDialog}
    </div>
  );
}
