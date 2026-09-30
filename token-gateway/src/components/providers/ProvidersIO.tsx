/**
 * 來源的匯出／匯入（對齊 CC Switch 的 import/export）。
 *
 * 兩個刻意的設計：
 *
 * 1. **匯出預設不含金鑰**。含金鑰是明碼，要使用者自己勾，勾了按鈕上就寫「含金鑰」，
 *    匯入時也會再提醒一次 —— 不讓「檔案裡有明碼金鑰」變成一件靜默的事。
 * 2. **匯入是 skip 語意**：同一個 `(name, base_url)` 已存在就跳過，不覆蓋既有設定。
 *    匯入不該是最容易弄壞現有東西的操作（後端 `providers_io.rs` 把這條寫成測試）。
 *
 * 讀檔用瀏覽器原生 `<input type="file">` ＋ `FileReader`：不必新增 Tauri 檔案對話框
 * 外掛（這個 repo 目前只有 `tauri-plugin-opener`，且建置走 `--offline`）。
 */
import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, type ImportReport } from "../../lib/api";
import { Icon } from "../icons";
import { Toggle } from "../Toggle";

export function ProvidersIO(props: { count: number }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [includeKeys, setIncludeKeys] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [path, setPath] = useState("");
  const [importPath, setImportPath] = useState("");
  const [report, setReport] = useState<ImportReport | null>(null);
  const [paste, setPaste] = useState("");
  const fileRef = useRef<HTMLInputElement | null>(null);

  const doExport = async () => {
    setBusy(true);
    setErr("");
    setMsg("");
    setPath("");
    try {
      const p = await api.providersExportFile(includeKeys);
      setPath(p);
      setMsg(
        `已匯出 ${props.count} 個來源${includeKeys ? "（含明碼金鑰，請妥善保管）" : "（不含金鑰）"}`,
      );
    } catch (e) {
      setErr(`匯出失敗：${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const copyJson = async () => {
    setBusy(true);
    setErr("");
    try {
      const json = await api.providersExport(includeKeys);
      await navigator.clipboard.writeText(json);
      setMsg(`已複製 ${props.count} 個來源的 JSON 到剪貼簿`);
      setPath("");
    } catch (e) {
      setErr(`複製失敗：${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const doImport = async (text: string) => {
    setBusy(true);
    setErr("");
    setMsg("");
    setReport(null);
    try {
      const rep = await api.providersImport(text);
      setReport(rep);
      void qc.invalidateQueries({ queryKey: ["providers"] });
      void qc.invalidateQueries({ queryKey: ["model_counts"] });
      setPaste("");
    } catch (e) {
      setErr(`匯入失敗：${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const doImportPath = async () => {
    setBusy(true);
    setErr("");
    setMsg("");
    setReport(null);
    try {
      const rep = await api.providersImportFile(importPath);
      setReport(rep);
      void qc.invalidateQueries({ queryKey: ["providers"] });
      void qc.invalidateQueries({ queryKey: ["model_counts"] });
    } catch (e) {
      setErr(`匯入失敗：${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const pickFile = (f: File | null) => {
    if (!f) return;
    const r = new FileReader();
    r.onload = () => void doImport(String(r.result ?? ""));
    r.onerror = () => setErr("讀取檔案失敗");
    r.readAsText(f, "utf-8");
  };

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className="btn-ghost flex items-center gap-1.5 px-4 py-1.5 text-sm"
          onClick={() => {
            setOpen(true);
            setReport(null);
            setErr("");
            setMsg("");
          }}
        >
          <Icon name="switch" size={13} />
          匯入／匯出
        </button>
        {msg && <span className="text-xs text-emerald-400/80">{msg}</span>}
        {err && <span className="break-all text-xs text-red-400">{err}</span>}
      </div>

      {open && (
        <div
          className="fixed inset-0 z-[90] flex justify-center overflow-y-auto p-4 glass-backdrop"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) setOpen(false);
          }}
        >
          <div className="my-auto w-full max-w-lg overflow-hidden glass-panel pop-in">
            <div className="flex items-center gap-3 border-b border-white/[0.06] bg-[rgba(24,24,30,0.98)] px-5 py-4">
              <button
                className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/[0.1] hover:text-white"
                onClick={() => setOpen(false)}
                title="關閉"
              >
                <Icon name="x" size={14} />
              </button>
              <div className="min-w-0 flex-1 truncate text-[15px] font-semibold text-white">
                來源匯入／匯出
              </div>
            </div>

            <div className="space-y-4 px-5 py-5">
              {/* ── 匯出 ── */}
              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  匯出
                </div>
                <p className="pb-2 text-xs leading-relaxed text-white/35">
                  匯出全部 {props.count} 個來源的連線資訊、已登記模型、每模型價格與訂閱時段
                  （不含用量紀錄）。
                </p>
                <div className="mac-frow">
                  <span className="mac-cap">包含金鑰</span>
                  <Toggle
                    checked={includeKeys}
                    onChange={setIncludeKeys}
                  />
                </div>
                {includeKeys && (
                  <p className="text-[11px] leading-relaxed text-amber-300/80">
                    ⚠︎ 金鑰會以明碼寫進匯出檔。只在你信任這個檔案存放位置時才打開。
                  </p>
                )}
                <div className="flex flex-wrap items-center justify-end gap-2 pt-2">
                  <button
                    className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
                    disabled={busy || props.count === 0}
                    onClick={() => void copyJson()}
                    title="把 JSON 複製到剪貼簿，自己貼到檔案裡"
                  >
                    <Icon name="copy" size={12} />
                    複製 JSON
                  </button>
                  <button
                    className="btn-primary flex items-center gap-1.5 px-4 py-1.5 text-xs disabled:opacity-40"
                    disabled={busy || props.count === 0}
                    onClick={() => void doExport()}
                  >
                    <Icon name="download" size={12} />
                    {includeKeys ? "匯出到檔案（含金鑰）" : "匯出到檔案"}
                  </button>
                </div>
                {path && (
                  <div className="mt-2 flex items-center gap-2 rounded-lg bg-white/[0.04] px-2.5 py-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-white/55" title={path}>
                      {path}
                    </span>
                    <button
                      className="btn-ghost shrink-0 px-2 py-1 text-[11px]"
                      onClick={() => void navigator.clipboard?.writeText(path)}
                    >
                      複製路徑
                    </button>
                  </div>
                )}
              </div>

              {/* ── 匯入 ── */}
              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  匯入
                </div>
                <p className="pb-2 text-xs leading-relaxed text-white/35">
                  同名且同上游地址的來源會被跳過（不覆蓋既有設定）。名稱若已有會自動加序號。
                </p>
                <div className="mac-frow">
                  <span className="mac-cap">檔案路徑</span>
                  <div className="flex flex-1 items-center gap-1.5">
                    <input
                      className="field w-full px-2.5 py-1.5 font-mono text-[11px]"
                      placeholder="貼上剛匯出的路徑，例如 …\Downloads\token-gateway-providers-….json"
                      value={importPath}
                      onChange={(e) => setImportPath(e.target.value)}
                    />
                    <button
                      className="btn-ghost flex shrink-0 items-center gap-1 px-3 py-1.5 text-xs disabled:opacity-40"
                      disabled={busy || importPath.trim().length === 0}
                      onClick={() => void doImportPath()}
                    >
                      <Icon name="upload" size={12} />
                      匯入
                    </button>
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-2 pt-2">
                  <input
                    ref={fileRef}
                    type="file"
                    accept="application/json,.json"
                    className="hidden"
                    onChange={(e) => {
                      pickFile(e.target.files?.[0] ?? null);
                      e.target.value = "";
                    }}
                  />
                  <button
                    className="btn-ghost flex items-center gap-1.5 px-4 py-1.5 text-xs disabled:opacity-40"
                    disabled={busy}
                    onClick={() => fileRef.current?.click()}
                  >
                    <Icon name="upload" size={12} />
                    選擇檔案…
                  </button>
                  <span className="text-[11px] text-white/25">或在下面直接貼上 JSON</span>
                </div>
                <textarea
                  className="field mt-2 h-24 w-full px-3 py-2 font-mono text-[11px]"
                  placeholder='{"kind":"token-gateway/providers","version":1,"providers":[…]}'
                  value={paste}
                  onChange={(e) => setPaste(e.target.value)}
                />
                <div className="flex justify-end pt-2">
                  <button
                    className="btn-ghost flex items-center gap-1.5 px-4 py-1.5 text-xs disabled:opacity-40"
                    disabled={busy || paste.trim().length === 0}
                    onClick={() => void doImport(paste)}
                  >
                    <Icon name="upload" size={12} />
                    貼上的內容匯入
                  </button>
                </div>
              </div>

              {report && (
                <div className="rounded-lg bg-white/[0.04] px-3 py-2.5 text-xs leading-relaxed text-white/70">
                  <div className="font-medium text-white/85">匯入完成</div>
                  <div className="pt-1">
                    新增 <span className="text-emerald-400">{report.added}</span> 個來源
                    （模型 {report.models}、價格 {report.pricing}、時段 {report.periods}）、
                    跳過 <span className="text-white/50">{report.skipped}</span> 個已存在
                  </div>
                  {report.names.length > 0 && (
                    <div className="truncate pt-1 font-mono text-[11px] text-white/40">
                      {report.names.join("、")}
                    </div>
                  )}
                  {report.includes_keys && (
                    <div className="pt-1 text-amber-300/85">
                      ⚠︎ 這個檔案帶有明碼金鑰，已一併寫入資料庫。
                    </div>
                  )}
                </div>
              )}

              {err && (
                <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs text-red-400">
                  {err}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
