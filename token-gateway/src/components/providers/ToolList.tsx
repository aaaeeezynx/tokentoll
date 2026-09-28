import type { ToolStatus } from "../../lib/api";
import { Logo } from "../logos";
import { Toggle } from "../Toggle";

/**
 * 「本機工具」清單：每個工具一張卡片（狀態、接管開關、Codex 體檢）。
 *
 * 由 `Providers.tsx` 抽出（該檔 2026-09-28 時 556 行）—— 原本這段是內嵌在
 * `view === "tools"` 分支裡的一大塊 JSX。純呈現：狀態與行為都由 props 帶入。
 * JSX 與文案一字未改。
 */
export function ToolList(props: {
  tools: ToolStatus[];
  isPending: boolean;
  error: unknown;
  /** 正在切換中的工具 app 名。 */
  toggling: string | null;
  rowErr: { app: string; msg: string } | null;
  rowMsg: { app: string; msg: string } | null;
  doctorLines: string[] | null;
  doctorBusy: boolean;
  onOpen: (t: ToolStatus) => void;
  onToggleRow: (t: ToolStatus, on: boolean) => void;
  onRunDoctor: () => void;
}) {
  const {
    tools,
    isPending,
    error,
    toggling,
    rowErr,
    rowMsg,
    doctorLines,
    doctorBusy,
  } = props;
  return (
    <>
      {isPending && <p className="text-sm text-white/30">檢測中…</p>}
      {props.error != null && (
        <p className="text-sm text-red-400">檢測失敗：{String(error)}</p>
      )}
      <div className="space-y-2">
        {tools.map((t, i) => (
          <div
            key={t.app}
            style={{ animationDelay: `${Math.min(i * 50, 300)}ms` }}
            onClick={() => props.onOpen(t)}
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
                      ? "關閉：還原接管前備份，並切回這個工具的原生來源"
                      : "開啟：直接接管（詳情點行左側）"
                    : "僅檢測，自動切換待支援"
                }
                onChange={(v) => props.onToggleRow(t, v)}
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
                  onClick={() => props.onRunDoctor()}
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
        Cursor／Antigravity／Hermes 不提供接管，只作為用量歸屬標籤出現在用量頁。
      </p>
    </>
  );
}
