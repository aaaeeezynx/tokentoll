import { useState } from "react";
import { api, type LocalKey } from "../../lib/api";
import { Icon } from "../icons";
import { copyText, fmtTokens } from "./shared";

/** 檢視 Key：顯示完整 Key（可複製）＋詳細資訊＋編輯入口。 */
export function ViewDialog(props: { item: LocalKey; onClose: () => void; onEdit: () => void }) {
  const k = props.item;
  const [revealed, setRevealed] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reveal = async () => {
    setLoading(true);
    setErr(null);
    try {
      setRevealed(await api.keyReveal(k.id));
    } catch (e) {
      setErr(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[85] flex items-center justify-center glass-backdrop p-4"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div className="w-full max-w-md glass-panel pop-in p-5">
        <div className="flex items-center gap-2.5">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/70">
            <Icon name="key" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[15px] font-semibold text-fg">{k.name}</div>
            <div className="truncate font-mono text-[11px] text-fg/30">
              {k.key_prefix} · {k.enabled ? "已啟用" : "已停用"}
            </div>
          </div>
          <button
            className="rounded-full bg-fg/[0.055] p-1.5 text-fg/60 transition-colors hover:bg-fg/10 hover:text-fg"
            onClick={props.onClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
        </div>

        <div className="mt-4 space-y-3 text-[13px]">
          <div className="rounded-xl border border-fg/[0.07] bg-black/20 p-3">
            <div className="mb-1.5 text-[11px] font-medium text-fg/40">完整 Key</div>
            {revealed ? (
              <>
                <code className="block max-h-28 select-all overflow-y-auto break-all rounded-lg bg-black/30 p-2.5 font-mono text-fg/90">
                  {revealed}
                </code>
                <div className="mt-2 flex gap-2">
                  <button
                    className="btn-primary flex items-center gap-1.5 px-3 py-1.5 text-xs"
                    onClick={() => copyText(revealed)}
                  >
                    <Icon name="copy" size={12} />
                    複製完整 Key
                  </button>
                  <button
                    className="btn-ghost px-3 py-1.5 text-xs"
                    onClick={() => setRevealed(null)}
                  >
                    隱藏
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="mb-2 text-fg/40">簽發後仍可隨時查看明文（本機存儲）。</p>
                {err && <p className="mb-2 text-red-400">{err}</p>}
                <button
                  className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
                  disabled={loading}
                  onClick={reveal}
                >
                  <Icon name="eye" size={12} />
                  {loading ? "載入中…" : "顯示完整 Key"}
                </button>
              </>
            )}
          </div>

          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">來源</span>
            <span className="text-fg/75">{k.provider_name || "未綁定"}</span>
          </div>
          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">用量</span>
            <span className="text-fg/75">
              {k.used_tokens.toLocaleString()} / {fmtTokens(k.quota_tokens)}
            </span>
          </div>
          <div className="mac-frow !grid-cols-[88px_1fr]">
            <span className="mac-cap">限流</span>
            <span className="text-fg/75">
              {k.rate_limit_qpm <= 0 ? "不限" : `${k.rate_limit_qpm}/分鐘`}
            </span>
          </div>

          {k.allowed_models.length > 0 && (
            <div>
              <div className="mb-1.5 text-[11px] font-medium text-fg/40">模型白名單</div>
              <div className="flex flex-wrap gap-1">
                {k.allowed_models.map((m) => (
                  <span
                    key={m}
                    className="rounded-full bg-fg/[0.06] px-2 py-0.5 font-mono text-[11px] text-fg/60"
                  >
                    {m}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button className="btn-ghost px-4 py-1.5 text-sm" onClick={props.onClose}>
            關閉
          </button>
          <button className="btn-primary px-4 py-1.5 text-sm" onClick={props.onEdit}>
            編輯
          </button>
        </div>
      </div>
    </div>
  );
}
