/**
 * 診斷頁的「協議記憶」區塊：網關學到「這個來源的這個模型在哪種端點上架」。
 *
 * 背景：opencode-go 這類來源的模型**逐模型**只在一種端點上架（grok-4.7 只在
 * /responses、mimo-v2.6 只在 /chat/completions），上游對錯的協議回
 * `400 ModelProtocolUnsupported`。網關換對協議後會記住，下次第一個就試它，
 * 而且落庫（`provider_model_protocol`），重啟不用重踩 400。
 */
import type { LearnedProtocol } from "../../lib/apiTypes";

export function ProtocolMemory({ list }: { list: LearnedProtocol[] }) {
  if (list.length === 0) {
    return (
      <p className="text-[13px] text-white/35">
        還沒有任何協議記憶 —— 表示所有來源的宣告協議都與實際上架情形一致
        （不需要換手），或還沒打過需要換手的模型。
      </p>
    );
  }
  return (
    <div className="space-y-1.5">
      {list.map((p) => (
        <div
          key={`${p.provider_id}:${p.model}:${p.declared_format}`}
          className="flex flex-wrap items-center gap-2 rounded-md bg-white/[0.04] px-3 py-2"
        >
          <span className="min-w-0 truncate text-[13px] text-white/80">
            {p.provider_name || (
              <span className="text-white/35">已刪除的渠道 #{p.provider_id}</span>
            )}
          </span>
          <span className="font-mono text-[10px] text-white/25">
            #{p.provider_id}
          </span>
          <span className="min-w-0 truncate font-mono text-[12px] text-white/60">
            {p.model}
          </span>
          <span className="flex flex-1 flex-wrap items-center gap-1.5">
            <span className="rounded bg-white/[0.06] px-1.5 py-px font-mono text-[11px] text-white/45">
              {p.declared_format}
            </span>
            <span className="text-white/30">→</span>
            <span
              className="rounded px-1.5 py-px font-mono text-[11px]"
              style={{ background: "rgba(48,209,88,0.14)", color: "#30d158" }}
            >
              {p.actual_format}
            </span>
          </span>
        </div>
      ))}
    </div>
  );
}
