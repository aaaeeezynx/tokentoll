/**
 * 系統匣切換的接收端（P1.6，對標 CC Switch 的托盤切換）。
 *
 * Rust 端的托盤只負責「選單長什麼樣」與「點擊變成事件」（見 `src-tauri/src/tray.rs`），
 * 實際寫入走這裡：用**與工具卡片同一份**請求組裝（`switchReq.ts`），所以
 * 「挑哪個模型、用哪把 Key」的規則只有一份。
 *
 * 這算不算「使用者的明確動作」？**算** —— 他是在托盤選單上特意選了那個來源。
 * 所以可以寫入設定；這與「只是點一下來源卡片」（那條路只提示、不寫檔，
 * 見 `useToolSwitch` 的說明與 docs/TESTING.md §0.5.2）不同。
 *
 * 為什麼掛在全域而不是來源頁：使用者可能在「用量」頁就把托盤切了 —— 掛在來源頁
 * 會讓那個動作靜默失敗。回饋用一個小 toast（這個 App 原本沒有 toast，
 * 但寫設定這種事不該靜默發生）。
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { api, useGatewayPort } from "../lib/api";
import { buildSelectionReq } from "./providers/switchReq";
import { saveLastSwitch } from "./providers/providersTypes";
import { Icon } from "./icons";

type TraySwitch = { app: string; provider_id: number };

export function TraySwitchListener() {
  const port = useGatewayPort();
  const qc = useQueryClient();
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const portRef = useRef(port);
  portRef.current = port;

  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    const run = async (p: TraySwitch) => {
      setMsg("");
      setErr("");
      try {
        const [providers, tools] = await Promise.all([
          api.providersList(),
          api.toolsDetect(portRef.current),
        ]);
        const provider = providers.find((x) => x.id === p.provider_id);
        if (!provider) throw new Error(`找不到來源 #${p.provider_id}`);
        const { req, upstreamKeyWritten } = await buildSelectionReq({
          app: p.app,
          provider,
          port: portRef.current,
          tools,
        });
        await api.switchApply(req, portRef.current);
        saveLastSwitch(p.app, req);
        const label = tools.find((t) => t.app === p.app)?.display ?? p.app;
        setMsg(
          `${label} 已切換到「${provider.name}」：${req.model}` +
            (upstreamKeyWritten ? "（直連上游 Key）" : ""),
        );
        void qc.invalidateQueries({ queryKey: ["tools", portRef.current] });
        void qc.invalidateQueries({ queryKey: ["providers"] });
      } catch (e) {
        setErr(`托盤切換失敗：${e instanceof Error ? e.message : String(e)}`);
      }
    };
    void listen<TraySwitch>("tray-switch", (ev) => void run(ev.payload)).then((f) => {
      if (cancelled) f();
      else un = f;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [qc]);

  // 8 秒後自動收起來（成功訊息不需要一直佔位；錯誤留著讓使用者看清楚）。
  useEffect(() => {
    if (!msg) return;
    const t = window.setTimeout(() => setMsg(""), 8000);
    return () => window.clearTimeout(t);
  }, [msg]);

  if (!msg && !err) return null;
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-[120] max-w-sm">
      <div className="glass-panel pop-in flex items-start gap-2.5 px-3.5 py-2.5">
        <Icon
          name={err ? "alert" : "check"}
          size={14}
          className={err ? "mt-0.5 text-red-400" : "mt-0.5 text-emerald-400"}
        />
        <div className="min-w-0 flex-1 text-[12px] leading-relaxed break-words text-fg/75">
          {err || msg}
        </div>
        <button
          className="pointer-events-auto shrink-0 rounded p-0.5 text-fg/35 hover:text-fg/80"
          onClick={() => {
            setMsg("");
            setErr("");
          }}
          title="關閉"
        >
          <Icon name="x" size={12} />
        </button>
      </div>
    </div>
  );
}
