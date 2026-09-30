/**
 * Deep Link 匯入確認（P4.1）。
 *
 * 點 `tokengateway://v1/import?…` 連結時，App 會打開（或已經開著就轉交），
 * 這裡負責**先顯示預覽再讓使用者決定**（cc-switch 也是這個流程）：
 * 型別、內容摘要、安全提醒、這次會影響什麼。
 *
 * 機密值（API Key）在預覽裡是遮罩過的 —— 完整值只會寫進資料庫，不會顯示在畫面上。
 */
import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api, type DeeplinkPreview } from "../lib/api";
import { Icon } from "./icons";

export function DeeplinkDialog() {
  const qc = useQueryClient();
  const [url, setUrl] = useState<string | null>(null);
  const [preview, setPreview] = useState<DeeplinkPreview | null>(null);
  const [err, setErr] = useState("");
  const [done, setDone] = useState("");
  const [busy, setBusy] = useState(false);
  const handled = useRef<Set<string>>(new Set());

  const open = async (link: string) => {
    if (handled.current.has(link)) return;
    handled.current.add(link);
    setDone("");
    setErr("");
    setPreview(null);
    setUrl(link);
    try {
      setPreview(await api.deeplinkPreview(link));
    } catch (e) {
      setErr(String(e));
    }
  };

  // 啟動時帶進來的連結（只取一次）
  useEffect(() => {
    void (async () => {
      try {
        const pending = await api.deeplinkTakePending();
        if (pending) await open(pending);
      } catch {
        /* 沒有 pending 就算了 */
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 執行中才送來的連結（已經開著時點連結）
  useEffect(() => {
    const un = listen<string>("deeplink", (e) => {
      if (typeof e.payload === "string") void open(e.payload);
    });
    return () => {
      void un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!url) return null;

  const close = () => {
    setUrl(null);
    setPreview(null);
    setErr("");
  };

  const apply = async () => {
    setBusy(true);
    try {
      const out = await api.deeplinkApply(url);
      setDone(
        `${out.note}` +
          (out.created.length > 0 ? `｜新增：${out.created.join("、")}` : "") +
          (out.skipped.length > 0 ? `｜已存在略過：${out.skipped.join("、")}` : ""),
      );
      setErr("");
      // 匯入可能動到四個地方，全部刷新
      for (const key of [
        "providers",
        "prompts_list",
        "mcp_list",
        "skills_list",
        "skills_repos",
      ]) {
        void qc.invalidateQueries({ queryKey: [key] });
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[95] flex justify-center overflow-y-auto p-4 glass-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div className="my-auto flex max-h-[min(92vh,760px)] w-full max-w-xl flex-col overflow-hidden glass-panel pop-in">
        <div className="flex shrink-0 items-center gap-3 border-b border-fg/[0.06] bg-[var(--app-elevated-solid)] px-5 py-4">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/70">
            <Icon name="download" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[15px] font-semibold text-fg">
              {preview ? preview.title : "Deep Link 匯入"}
            </div>
            <div className="text-[11px] text-fg/30">
              來自連結的設定 —— 確認後才會寫入
            </div>
          </div>
          <button
            className="rounded-full bg-fg/[0.06] p-1.5 text-fg/55 transition-colors hover:bg-fg/[0.1] hover:text-fg"
            onClick={close}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {!preview && !err && <p className="text-[13px] text-fg/40">解析中…</p>}
          {err && (
            <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs break-words text-red-400">
              {err}
            </div>
          )}

          {preview && (
            <>
              <div className="space-y-1">
                {preview.fields.map(([k, v]) => (
                  <div key={k} className="flex gap-3 text-[12px]">
                    <span className="w-20 shrink-0 text-fg/35">{k}</span>
                    <span className="min-w-0 flex-1 break-words whitespace-pre-wrap text-fg/75">
                      {v}
                    </span>
                  </div>
                ))}
              </div>

              {preview.effects.length > 0 && (
                <div className="rounded-lg bg-fg/[0.03] px-3 py-2">
                  <div className="pb-1 text-[11px] text-fg/40">這次會做的事</div>
                  {preview.effects.map((e) => (
                    <div key={e} className="text-[12px] text-fg/65">
                      · {e}
                    </div>
                  ))}
                </div>
              )}

              {preview.warnings.length > 0 && (
                <div className="rounded-lg border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2">
                  {preview.warnings.map((w) => (
                    <div key={w} className="text-[12px] text-amber-300/90">
                      ⚠ {w}
                    </div>
                  ))}
                </div>
              )}

              <div className="rounded-lg bg-black/25 px-3 py-2 font-mono text-[10px] break-all text-fg/25">
                {url}
              </div>
            </>
          )}

          {done && (
            <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/[0.06] px-3 py-2 text-[12px] break-words text-emerald-300/90">
              {done}
            </div>
          )}
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-fg/[0.06] px-5 py-3">
          <button className="btn-ghost px-5 py-1.5 text-[13px]" onClick={close}>
            {done ? "關閉" : "取消"}
          </button>
          {!done && (
            <button
              className="btn-primary px-6 py-1.5 text-[13px] disabled:opacity-40"
              disabled={!preview || busy}
              onClick={() => void apply()}
            >
              {busy ? "匯入中…" : "確認匯入"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
