/**
 * 來源連線檢查（P1.5，對標 CC Switch 的 connectivity check）。
 *
 * 語意與 CC Switch 一致：**只測位址可不可達，不送模型請求、不帶金鑰**。
 * 所以 401／403／404 都顯示為「可達」—— 它們證明主機活著，只是沒有金鑰或那個
 * 路徑本來就沒有端點。畫面下方會把這句話講清楚，免得使用者以為「檢查過了就一
 * 定能用」（CC Switch 的 FAQ 也特別提醒同一件事）。
 */
import { useState } from "react";
import { api, type CheckResult } from "../../lib/api";
import { Icon } from "../icons";

export function ConnectivityCheck(props: { baseUrl: string }) {
  const [res, setRes] = useState<CheckResult | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    setErr("");
    try {
      setRes(await api.providerCheck(props.baseUrl));
    } catch (e) {
      setRes(null);
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-3 rounded-lg bg-white/[0.03] px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <button
          className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
          disabled={busy}
          onClick={() => void run()}
          title="只檢查位址可不可達，不會送模型請求、也不會帶上你的金鑰"
        >
          <Icon name="bolt-fill" size={11} />
          {busy ? "檢查中…" : "檢查連線"}
        </button>

        {res && (
          <span className="flex flex-wrap items-center gap-2 text-[11px]">
            <span
              className="rounded px-1.5 py-px"
              style={
                res.reachable
                  ? { background: "rgba(48,209,88,0.14)", color: "#30d158" }
                  : { background: "rgba(255,69,58,0.16)", color: "#ff6961" }
              }
            >
              {res.reachable ? "可達" : "連不上"}
            </span>
            <span className="text-white/45">{res.message}</span>
            <span className="text-white/25">{res.latency_ms} ms</span>
          </span>
        )}
        {err && <span className="text-[11px] text-red-400">{err}</span>}
      </div>

      {res && (
        <div className="pt-1.5 font-mono text-[10px] break-all text-white/25">
          GET {res.url}
        </div>
      )}
      <p className="pt-1.5 text-[11px] leading-relaxed text-white/25">
        只檢查位址可不可達（不送模型請求、不帶金鑰），所以 401／403／404 都算「可達」。
        金鑰與模型名對不對，要真的發一次請求才知道 —— 檢查過了不等於一定能用。
      </p>
    </div>
  );
}
