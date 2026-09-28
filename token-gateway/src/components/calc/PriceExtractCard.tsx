import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, type PriceExtractResult, type UsableModel } from "../../lib/api";
import { PopSelect } from "../PopSelect";
import { Segmented } from "../Segmented";

/** 價格對比：網站/截圖抽取值 vs 來源內已存同模型定價（只對比，不寫庫）。 */
export function PriceExtractCard() {
  // 解析用（調哪個來源的哪個模型來做抽取）
  const [xPid, setXPid] = useState("");
  const [xModel, setXModel] = useState("");
  // 對比對象：已儲存的所有同名模型（跨來源＋全域）
  const [cModel, setCModel] = useState("");
  const [sort, setSort] = useState<"cheap" | "expensive">("cheap");
  const [url, setUrl] = useState("");
  const [pasted, setPasted] = useState("");
  const [imgB64, setImgB64] = useState<string | null>(null);
  const [imgName, setImgName] = useState("");
  const [err, setErr] = useState("");
  const providers = useQuery({ queryKey: ["providers"], queryFn: api.providersList });
  const xModels = useQuery({
    queryKey: ["umodels", xPid || "none"],
    queryFn: () => api.modelsList(Number(xPid)),
    enabled: xPid !== "",
  });
  const xModelIds = (xModels.data || []).map((m: UsableModel) => m.actual_model);
  const inputCls = "field w-full px-3 py-1.5 text-xs";

  interface StoredRow {
    key: string;
    label: string;
    sub: string;
    in_pm: number;
    out_pm: number;
    cache_read_pm: number;
    cache_create_pm: number;
  }

  const run = useMutation({
    mutationFn: async () => {
      const ext = await api.priceExtract(Number(xPid), xModel.trim(), {
        url: url.trim() || null,
        pastedText: pasted.trim() || null,
        imageBase64: imgB64,
        targetHint: cModel.trim() || null,
      });
      const norm = cModel.trim().toLowerCase();
      const [pp, gl] = await Promise.all([api.ppList(null), api.pricingList()]);
      const rows: StoredRow[] = [
        ...pp
          .filter((p) => p.model_norm === norm)
          .map((p) => ({
            key: `pp-${p.provider_id}`,
            label: p.provider_name,
            sub: `${p.mode} · ${p.source}`,
            in_pm: p.in_pm,
            out_pm: p.out_pm,
            cache_read_pm: p.cache_read_pm,
            cache_create_pm: p.cache_create_pm,
          })),
        ...gl
          .filter((g) => g.model_norm === norm)
          .map((g) => ({
            key: "global",
            label: "全域預設",
            sub: g.source,
            in_pm: g.in_pm,
            out_pm: g.out_pm,
            cache_read_pm: g.cache_read_pm,
            cache_create_pm: g.cache_create_pm,
          })),
      ];
      return { ext, rows, norm };
    },
    onSuccess: () => setErr(""),
    onError: (e) => setErr(String(e)),
  });
  const ext: PriceExtractResult | undefined = run.data?.ext;
  const rows: StoredRow[] = run.data?.rows ?? [];
  const trial = (r: { in_pm: number; out_pm: number }) => r.in_pm + r.out_pm; // 1M 輸入＋1M 輸出試算
  const sorted = useMemo(() => {
    const arr = [...rows];
    arr.sort((a, b) => (sort === "cheap" ? trial(a) - trial(b) : trial(b) - trial(a)));
    return arr;
  }, [rows, sort]);

  const onFile = (f: File | undefined) => {
    if (!f) {
      setImgB64(null);
      setImgName("");
      return;
    }
    if (f.size > 5 * 1024 * 1024) {
      setErr("截圖過大（限 5MB），請壓縮後重試");
      return;
    }
    const rd = new FileReader();
    rd.onload = () => {
      const s = String(rd.result || "");
      const comma = s.indexOf(",");
      setImgB64(comma >= 0 ? s.slice(comma + 1) : s);
      setImgName(f.name);
      setErr("");
    };
    rd.onerror = () => setErr("讀取圖片失敗");
    rd.readAsDataURL(f);
  };

  const canRun =
    xPid !== "" &&
    xModel.trim() !== "" &&
    cModel.trim() !== "" &&
    (url.trim() !== "" || pasted.trim() !== "" || imgB64 !== null);

  return (
    <div className="glass p-5">
      <div className="mb-1 text-sm font-semibold tracking-tight text-white/80">網站價格對比</div>
      <p className="mb-3 text-[11px] text-white/30">
        解析用來源內模型讀網站/截圖，對比該模型名在所有來源＋全域的已存定價。只對比不寫庫，抽取不計入用量。
      </p>
      <div className="grid gap-x-4 md:grid-cols-2">
        <div className="mac-frow">
          <span className="mac-cap">解析用來源</span>
          <PopSelect
            value={xPid}
            onChange={(v) => { setXPid(v); setXModel(""); }}
            options={[
              ...(providers.data || [])
                .filter((p) => ["openai-chat", "openai-responses", "mixed"].includes(p.api_format))
                .map((p) => ({ value: String(p.id), label: p.name, hint: p.base_url })),
            ]}
          />
        </div>
        <div className="mac-frow">
          <span className="mac-cap">解析用模型</span>
          {xModelIds.length > 0 ? (
            <PopSelect value={xModel} onChange={setXModel} options={xModelIds.map((id) => ({ value: id, label: id }))} />
          ) : (
            <input className={inputCls} placeholder="如 gpt-4o-mini（OpenAI 兼容來源的模型）" value={xModel} onChange={(e) => setXModel(e.target.value)} />
          )}
        </div>
      </div>
      <div className="mac-frow">
        <span className="mac-cap">對比模型名</span>
        <input className={inputCls} placeholder="已存同名模型，如 deepseek-v4-flash（比對所有來源＋全域）" value={cModel} onChange={(e) => setCModel(e.target.value)} />
      </div>
      <div className="mac-frow">
        <span className="mac-cap">定價網址</span>
        <input className={inputCls} placeholder="如 https://aihubmix.com/model/deepseek-v4-flash-0731#pricing（後端代抓）" value={url} onChange={(e) => setUrl(e.target.value)} />
      </div>
      <div className="mac-frow items-start">
        <span className="mac-cap pt-1.5">貼上文字</span>
        <textarea className={`${inputCls} min-h-[72px] resize-y`} placeholder="複製定價頁的價格段落貼到這裡（網址、文字、截圖三選一即可）" value={pasted} onChange={(e) => setPasted(e.target.value)} />
      </div>
      <div className="mac-frow">
        <span className="mac-cap">截圖</span>
        <div className="flex flex-1 items-center gap-2">
          <label className="btn-ghost cursor-pointer px-3 py-1.5 text-xs">
            選擇圖片
            <input type="file" accept="image/*" className="hidden" onChange={(e) => onFile(e.target.files?.[0])} />
          </label>
          <span className="truncate text-[11px] text-white/35">{imgName || "未選擇（可選，需 vision 模型）"}</span>
          {imgB64 && <button className="btn-ghost px-2 py-1 text-[11px]" onClick={() => onFile(undefined)}>清除</button>}
        </div>
      </div>
      {err && <p className="mt-2 text-xs text-red-400">{err}</p>}
      <div className="mt-2 flex items-center gap-2">
        <button className="btn-primary px-5 py-2 text-sm disabled:opacity-40" disabled={!canRun || run.isPending} onClick={() => run.mutate()}>
          {run.isPending ? "對比中…" : "開始對比"}
        </button>
        <span className="text-[11px] text-white/25">解析用 Key 只在後端使用；只有 OpenAI 兼容來源可做解析。</span>
      </div>
      {ext && (
        <div className="anim-rise mt-3 space-y-3">
          <div className="rounded-xl bg-white/[0.03] p-3.5 ring-1 ring-white/10">
            <div className="mb-2 flex flex-wrap items-center gap-2 text-xs font-semibold text-white/70">
              <span>網站抽取值</span>
              <span className="font-normal text-white/30">· 經 {ext.used_model} · 信心 {(ext.confidence * 100).toFixed(0)}% · {ext.currency}</span>
              <span className="font-mono font-normal text-white/50">輸入 ${ext.in_pm} / 輸出 ${ext.out_pm} / 讀 ${ext.cache_read_pm} / 建 ${ext.cache_create_pm} · 1M＋1M 試算 ${trial(ext).toFixed(4)}</span>
            </div>
            {ext.source_note && <div className="mt-1 text-[11px] text-white/30">{ext.source_note}</div>}
            <div className="mt-1 text-[11px] text-white/40">證據：{ext.evidence}</div>
            {ext.confidence < 0.5 && <div className="mt-1 text-[11px] text-amber-300/80">信心偏低，請對照原文核對。</div>}
          </div>
          <div className="rounded-xl bg-white/[0.03] p-3.5 ring-1 ring-white/10">
            <div className="mb-2 flex items-center gap-2">
              <div className="text-xs font-semibold text-white/70">已存同名模型（{sorted.length}）</div>
              <span className="flex-1" />
              <Segmented
                value={sort}
                onChange={(v) => setSort(v as "cheap" | "expensive")}
                btnClassName="px-2.5 py-1 text-[11px] font-medium"
                options={[{ id: "cheap", label: "便宜優先" }, { id: "expensive", label: "貴優先" }]}
              />
            </div>
            {sorted.length === 0 ? (
              <div className="text-xs text-white/30">無已存同名定價（模型名需與來源/全域定價表完全一致，小寫）。</div>
            ) : (
              <div className="divide-y divide-white/[0.06]">
                {sorted.map((s) => {
                  const d = trial(ext) - trial(s);
                  const cls = d === 0 ? "text-white/40" : d > 0 ? "text-[#30d158]" : "text-amber-300";
                  const txt = d === 0 ? "持平" : `${d > 0 ? "已存便宜" : "網站便宜"} ${Math.abs(d).toFixed(4)}`;
                  return (
                    <div key={s.key} className="flex items-center gap-2 py-1.5 text-xs">
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-white/80">{s.label}</div>
                        <div className="truncate font-mono text-[10px] text-white/30">
                          {s.sub} · ${s.in_pm}/${s.out_pm}/${s.cache_read_pm}/${s.cache_create_pm}
                        </div>
                      </div>
                      <span className="shrink-0 font-mono text-white/60">1M＋1M ${trial(s).toFixed(4)}</span>
                      <span className={`shrink-0 font-mono ${cls}`}>{txt}</span>
                    </div>
                  );
                })}
              </div>
            )}
            <div className="mt-1 text-[11px] text-white/25">排序基準：1M 輸入＋1M 輸出試算價。</div>
          </div>
        </div>
      )}
    </div>
  );
}
