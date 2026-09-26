import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, type PriceExtractResult, type PriceQuote, type UsableModel, type UsageSummary } from "../lib/api";
import { Icon } from "./icons";
import { PopSelect } from "./PopSelect";
import { RangePicker, type CustomRange } from "./RangePicker";
import { Segmented } from "./Segmented";

/** 價格對比：網站/截圖抽取值 vs 來源內已存同模型定價（只對比，不寫庫）。 */
function PriceExtractCard() {
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

const SCENARIOS = [
  { id: "chat", label: "閒聊", inTok: 5000, outTok: 1000, cr: 0, cw: 0 },
  { id: "code", label: "程式", inTok: 50000, outTok: 10000, cr: 200000, cw: 5000 },
  { id: "doc", label: "長文", inTok: 200000, outTok: 5000, cr: 500000, cw: 0 },
  { id: "w7d", label: "近7天用量", inTok: 0, outTok: 0, cr: 0, cw: 0 },
  { id: "w30d", label: "近30天用量", inTok: 0, outTok: 0, cr: 0, cw: 0 },
  { id: "dates", label: "指定日期", inTok: 0, outTok: 0, cr: 0, cw: 0 },
] as const;

function fillFromSummary(s: UsageSummary) {
  return { inTok: s.in_tok, outTok: s.out_tok, cr: s.cache_read, cw: s.cache_write };
}

interface ManualRates {
  in_pm: number;
  out_pm: number;
  cache_read_pm: number;
  cache_create_pm: number;
}

function manualQuote(r: ManualRates, t: { inTok: number; outTok: number; cr: number; cw: number }): PriceQuote {
  const cost =
    (t.inTok / 1e6) * r.in_pm +
    (t.outTok / 1e6) * r.out_pm +
    (t.cr / 1e6) * r.cache_read_pm +
    (t.cw / 1e6) * r.cache_create_pm;
  return {
    cost_usd: cost,
    in_pm: r.in_pm,
    out_pm: r.out_pm,
    cache_read_pm: r.cache_read_pm,
    cache_create_pm: r.cache_create_pm,
    source: "手填費率",
    mode: "usage",
    sub_fee_usd: 0,
    sub_included_tokens: -1,
    sub_used_this_month: 0,
    sub_over_tokens: 0,
  };
}

/** 手填訂閱試算（額度視為全新未用；抵扣順序同後端）。 */
function manualSubQuote(
  fee: number,
  included: number,
  r: ManualRates,
  t: { inTok: number; outTok: number; cr: number; cw: number },
): PriceQuote {
  let rem = included < 0 ? Number.MAX_SAFE_INTEGER : Math.max(0, included);
  let over = 0;
  let overTok = 0;
  const parts: [number, number][] = [
    [t.inTok, r.in_pm],
    [t.outTok, r.out_pm],
    [t.cr, r.cache_read_pm],
    [t.cw, r.cache_create_pm],
  ];
  for (const [amt, rate] of parts) {
    const a = Math.max(0, amt);
    const take = Math.min(rem, a);
    rem -= take;
    overTok += a - take;
    over += ((a - take) / 1e6) * rate;
  }
  return {
    cost_usd: fee + over,
    in_pm: r.in_pm,
    out_pm: r.out_pm,
    cache_read_pm: r.cache_read_pm,
    cache_create_pm: r.cache_create_pm,
    source: "手填費率",
    mode: "subscription",
    sub_fee_usd: fee,
    sub_included_tokens: included,
    sub_used_this_month: 0,
    sub_over_tokens: overTok,
  };
}

function fmtCost(n: number): string {
  return n < 0.01 ? n.toFixed(6) : n.toFixed(4);
}

function SideCard(props: {
  title: string;
  quote: PriceQuote | null;
  pending: boolean;
  winner: boolean;
  modelLabel: string;
}) {
  const q = props.quote;
  return (
    <div
      className={`glass-float p-4 ${props.winner ? "ring-1 ring-[#30d158]/40" : ""}`}
    >
      <div className="flex items-center gap-2">
        <div className="text-sm font-semibold tracking-tight text-white/85">
          {props.title}
        </div>
        {props.winner && (
          <span className="rounded-full bg-[#30d158]/15 px-2 py-0.5 text-[10px] font-medium text-[#30d158]">
            更便宜
          </span>
        )}
      </div>
      <div className="mt-0.5 truncate font-mono text-[11px] text-white/35">
        {props.modelLabel}
      </div>
      {props.pending ? (
        <p className="mt-2 text-sm text-white/30">試算中…</p>
      ) : q ? (
        <>
          <div className="mt-2 text-2xl font-semibold tracking-tight text-white">
            ${fmtCost(q.cost_usd)}
          </div>
          {q.mode === "subscription" ? (
            <div className="mt-2 space-y-1.5 text-xs text-white/45">
              <div className="flex justify-between">
                <span>月費</span>
                <span className="font-mono">${q.sub_fee_usd}</span>
              </div>
              <div>
                <div className="flex justify-between text-[11px]">
                  <span>額度</span>
                  <span className="font-mono">
                    {q.sub_used_this_month.toLocaleString()} /{" "}
                    {q.sub_included_tokens < 0
                      ? "無限"
                      : q.sub_included_tokens.toLocaleString()}
                  </span>
                </div>
                {q.sub_included_tokens >= 0 && (
                  <div className="mt-1 h-1 overflow-hidden rounded-full bg-white/[0.06]">
                    <div
                      className="h-full rounded-full bg-(--mac-accent)/60 bar-anim"
                      style={{
                        width: `${Math.min(100, (q.sub_used_this_month / Math.max(1, q.sub_included_tokens)) * 100)}%`,
                      }}
                    />
                  </div>
                )}
              </div>
              <div className="flex justify-between">
                <span>超額 {q.sub_over_tokens.toLocaleString()} tok</span>
                <span className="font-mono">
                  ${fmtCost(q.cost_usd - q.sub_fee_usd)}
                </span>
              </div>
              <div className="text-[11px] text-white/30">
                超額基礎價 {q.in_pm}/{q.out_pm} · 費率來源：{q.source}
              </div>
            </div>
          ) : (
            <div className="mt-2 space-y-1 text-xs text-white/45">
              <div className="flex justify-between">
                <span>輸入 ${q.in_pm}/M</span>
              </div>
              <div className="flex justify-between">
                <span>輸出 ${q.out_pm}/M</span>
              </div>
              <div className="flex justify-between">
                <span>
                  快取讀 ${q.cache_read_pm}/M · 建 ${q.cache_create_pm}/M
                </span>
              </div>
              <div className="text-[11px] text-white/30">費率來源：{q.source}</div>
            </div>
          )}
        </>
      ) : (
        <p className="mt-2 text-sm text-white/25">尚未試算</p>
      )}
    </div>
  );
}

export default function CalcPage() {
  const [scenario, setScenario] = useState<string>("code");
  const [toks, setToks] = useState({ inTok: 50000, outTok: 10000, cr: 200000, cw: 5000 });
  const [dateRange, setDateRange] = useState<CustomRange | null>(null);

  // 真實用量代入
  const sum7 = useQuery({
    queryKey: ["usage_summary", "7d", null, null, null],
    queryFn: () => api.usageSummary("7d", null, null, null),
  });
  const sum30 = useQuery({
    queryKey: ["usage_summary", "30d", null, null, null],
    queryFn: () => api.usageSummary("30d", null, null, null),
  });
  const sumCustom = useQuery({
    queryKey: ["usage_summary", "dates", dateRange?.start ?? null, dateRange?.end ?? null],
    queryFn: () =>
      api.usageSummary("30d", null, null, null, dateRange!.start, dateRange!.end),
    enabled: dateRange !== null,
  });
  useEffect(() => {
    if (sumCustom.data && dateRange) {
      setToks(fillFromSummary(sumCustom.data));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sumCustom.data]);

  // A 側：已存來源＋模型
  const [aPid, setAPid] = useState("");
  const [aModel, setAModel] = useState("");
  const providers = useQuery({ queryKey: ["providers"], queryFn: api.providersList });
  const aModels = useQuery({
    queryKey: ["umodels", aPid || "none"],
    queryFn: () => api.modelsList(Number(aPid)),
    enabled: aPid !== "",
  });
  const aModelIds = (aModels.data || []).map((m: UsableModel) => m.actual_model);

  // B 側：已存 或 手填（手填再分按量/訂閱）
  const [bMode, setBMode] = useState<"saved" | "manual">("saved");
  const [bBill, setBBill] = useState<"usage" | "subscription">("usage");
  const [subFee, setSubFee] = useState(0);
  const [subIncluded, setSubIncluded] = useState(-1);
  const [bPid, setBPid] = useState("");
  const [bModel, setBModel] = useState("");
  const bModels = useQuery({
    queryKey: ["umodels", bPid || "none"],
    queryFn: () => api.modelsList(Number(bPid)),
    enabled: bPid !== "" && bMode === "saved",
  });
  const bModelIds = (bModels.data || []).map((m: UsableModel) => m.actual_model);
  const [rates, setRates] = useState<ManualRates>({ in_pm: 0, out_pm: 0, cache_read_pm: 0, cache_create_pm: 0 });

  const [result, setResult] = useState<{
    a: PriceQuote;
    b: PriceQuote;
    aLabel: string;
    bLabel: string;
  } | null>(null);
  const [err, setErr] = useState("");

  const num = (v: string) => Math.max(0, Number(v) || 0);
  const inputCls = "field w-full px-3 py-1.5 text-xs";

  const pickScenario = (id: string) => {
    setScenario(id);
    if (id !== "dates") setDateRange(null);
    if (id === "w7d" && sum7.data) {
      setToks(fillFromSummary(sum7.data));
      return;
    }
    if (id === "w30d" && sum30.data) {
      setToks(fillFromSummary(sum30.data));
      return;
    }
    if (id === "dates") return;
    const s = SCENARIOS.find((x) => x.id === id);
    if (s) {
      setToks({ inTok: s.inTok, outTok: s.outTok, cr: s.cr, cw: s.cw });
    }
  };

  const pickDates = (c: CustomRange | null) => {
    setDateRange(c);
    if (c) setScenario("dates");
  };

  const run = useMutation({
    mutationFn: async () => {
      const a = await api.priceQuote(
        aPid === "" ? null : Number(aPid),
        aModel.trim(),
        toks.inTok,
        toks.outTok,
        toks.cr,
        toks.cw,
      );
      const b =
        bMode === "saved"
          ? await api.priceQuote(
              bPid === "" ? null : Number(bPid),
              bModel.trim(),
              toks.inTok,
              toks.outTok,
              toks.cr,
              toks.cw,
            )
          : bBill === "usage"
            ? manualQuote(rates, toks)
            : manualSubQuote(subFee, subIncluded, rates, toks);
      return { a, b };
    },
    onSuccess: ({ a, b }) => {
      setErr("");
      setResult({
        a,
        b,
        aLabel: `${aPid === "" ? "全域" : providers.data?.find((p) => String(p.id) === aPid)?.name ?? ""} / ${aModel.trim() || "—"}`,
        bLabel:
          bMode === "saved"
            ? `${bPid === "" ? "全域" : providers.data?.find((p) => String(p.id) === bPid)?.name ?? ""} / ${bModel.trim() || "—"}`
            : "手填費率",
      });
    },
    onError: (e) => setErr(String(e)),
  });

  const canRun =
    aModel.trim() !== "" && (bMode === "manual" || bModel.trim() !== "");
  const saving = useMemo(() => {
    if (!result || result.a.cost_usd === result.b.cost_usd) return null;
    const cheap = result.a.cost_usd < result.b.cost_usd ? "A" : "B";
    const hi = Math.max(result.a.cost_usd, result.b.cost_usd);
    const lo = Math.min(result.a.cost_usd, result.b.cost_usd);
    const pct = hi > 0 ? ((hi - lo) / hi) * 100 : 0;
    return { cheap, diff: hi - lo, pct };
  }, [result]);

  const rateField = (
    label: string,
    v: number,
    set: (n: number) => void,
  ) => (
    <div className="mac-frow" key={label}>
      <span className="mac-cap">{label}</span>
      <input
        className={inputCls}
        value={v}
        onChange={(e) => set(num(e.target.value))}
      />
    </div>
  );

  return (
    <div className="space-y-4">
      <PriceExtractCard />
      {/* ── 用量場景 ── */}
      <div className="glass p-5">
        <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">
          用量場景
        </div>
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <div className="min-w-0 flex-1">
            <Segmented
              className="w-full"
              value={scenario}
              onChange={pickScenario}
              btnClassName="px-2 py-1.5 text-xs font-medium"
              options={SCENARIOS.filter((s) => s.id !== "dates").map((s) => ({
                id: s.id,
                label: s.label,
              }))}
            />
          </div>
          <RangePicker
            presets={[]}
            range="30d"
            custom={dateRange}
            onPreset={() => {}}
            onCustom={pickDates}
            btnClassName="px-3 py-1.5 text-xs font-medium"
          />
        </div>
        <div className="grid grid-cols-2 gap-x-4 md:grid-cols-4">
          {(
            [
              ["inTok", "輸入 tokens"],
              ["outTok", "輸出 tokens"],
              ["cr", "快取讀 tokens"],
              ["cw", "快取建 tokens"],
            ] as const
          ).map(([k, label]) => (
            <label key={k} className="mb-1 block text-[11px] text-white/40">
              {label}
              <input
                className="field mt-1 w-full px-2.5 py-1.5 text-xs"
                value={toks[k]}
                onChange={(e) => {
                  setToks((t) => ({ ...t, [k]: num(e.target.value) }));
                  setScenario("custom");
                }}
              />
            </label>
          ))}
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        {/* ── A 側 ── */}
        <div className="glass p-5">
          <div className="mb-3 text-sm font-semibold tracking-tight text-white/80">
            A · 已存來源
          </div>
          <div className="mac-frow">
            <span className="mac-cap">來源</span>
            <PopSelect
              value={aPid}
              onChange={(v) => {
                setAPid(v);
                setAModel("");
              }}
              options={[
                { value: "", label: "全域預設" },
                ...(providers.data || []).map((p) => ({
                  value: String(p.id),
                  label: p.name,
                  hint: p.base_url,
                })),
              ]}
            />
          </div>
          <div className="mac-frow">
            <span className="mac-cap">模型</span>
            {aModelIds.length > 0 ? (
              <PopSelect
                value={aModel}
                onChange={setAModel}
                options={aModelIds.map((id) => ({ value: id, label: id }))}
              />
            ) : (
              <input
                className={inputCls}
                placeholder="模型 ID（小寫）"
                value={aModel}
                onChange={(e) => setAModel(e.target.value)}
              />
            )}
          </div>
        </div>

        {/* ── B 側 ── */}
        <div className="glass p-5">
          <div className="mb-3 flex items-center gap-2">
            <div className="text-sm font-semibold tracking-tight text-white/80">
              B · 對比方
            </div>
            <span className="flex-1" />
            <Segmented
              value={bMode}
              onChange={(v) => setBMode(v as "saved" | "manual")}
              btnClassName="px-2.5 py-1 text-xs font-medium"
              options={[
                { id: "saved", label: "已存來源" },
                { id: "manual", label: "手填費率" },
              ]}
            />
          </div>
          {bMode === "saved" ? (
            <>
              <div className="mac-frow">
                <span className="mac-cap">來源</span>
                <PopSelect
                  value={bPid}
                  onChange={(v) => {
                    setBPid(v);
                    setBModel("");
                  }}
                  options={[
                    { value: "", label: "全域預設" },
                    ...(providers.data || []).map((p) => ({
                      value: String(p.id),
                      label: p.name,
                      hint: p.base_url,
                    })),
                  ]}
                />
              </div>
              <div className="mac-frow">
                <span className="mac-cap">模型</span>
                {bModelIds.length > 0 ? (
                  <PopSelect
                    value={bModel}
                    onChange={setBModel}
                    options={bModelIds.map((id) => ({ value: id, label: id }))}
                  />
                ) : (
                  <input
                    className={inputCls}
                    placeholder="模型 ID（小寫）"
                    value={bModel}
                    onChange={(e) => setBModel(e.target.value)}
                  />
                )}
              </div>
            </>
          ) : (
            <>
              <div className="mb-1">
                <Segmented
                  className="w-full"
                  value={bBill}
                  onChange={(v) => setBBill(v as "usage" | "subscription")}
                  btnClassName="px-2 py-1 text-[11px] font-medium"
                  options={[
                    { id: "usage", label: "按量" },
                    { id: "subscription", label: "訂閱" },
                  ]}
                />
              </div>
              {bBill === "subscription" && (
                <>
                  <div className="mac-frow">
                    <span className="mac-cap">月費 $</span>
                    <input
                      className={inputCls}
                      value={subFee}
                      onChange={(e) => setSubFee(num(e.target.value))}
                    />
                  </div>
                  <div className="mac-frow">
                    <span className="mac-cap">免費額度</span>
                    <input
                      className={inputCls}
                      placeholder="tokens，-1 無限（視為全新未用）"
                      value={subIncluded}
                      onChange={(e) =>
                        setSubIncluded(Number(e.target.value) || 0)
                      }
                    />
                  </div>
                </>
              )}
              {rateField(
                bBill === "subscription" ? "超額輸入 $/M" : "輸入 $/M",
                rates.in_pm,
                (n) => setRates((r) => ({ ...r, in_pm: n })),
              )}
              {rateField(
                bBill === "subscription" ? "超額輸出 $/M" : "輸出 $/M",
                rates.out_pm,
                (n) => setRates((r) => ({ ...r, out_pm: n })),
              )}
              {rateField("快取讀 $/M", rates.cache_read_pm, (n) =>
                setRates((r) => ({ ...r, cache_read_pm: n })),
              )}
              {rateField("快取建 $/M", rates.cache_create_pm, (n) =>
                setRates((r) => ({ ...r, cache_create_pm: n })),
              )}
            </>
          )}
        </div>
      </div>

      {err && <p className="text-xs text-red-400">{err}</p>}
      <div className="flex items-center gap-2">
        <button
          className="btn-primary px-5 py-2 text-sm disabled:opacity-40"
          disabled={!canRun || run.isPending}
          onClick={() => run.mutate()}
        >
          {run.isPending ? "試算中…" : "開始比價"}
        </button>
        <span className="text-[11px] text-white/25">
          訂閱超額按基礎價，剩餘額度按輸入→輸出→快取讀→快取建抵扣；峰谷按當下時段估算
        </span>
      </div>

      {result && (
        <div className="anim-rise space-y-3">
          {saving && (
            <div className="glass flex items-center gap-2 border-[#30d158]/25 p-4">
              <Icon name="check" size={15} className="shrink-0 text-[#30d158]" />
              <span className="text-sm text-white/80">
                {saving.cheap} 更便宜，省 ${saving.diff.toFixed(4)}（
                {saving.pct.toFixed(1)}%）
              </span>
            </div>
          )}
          {!saving && (
            <div className="glass p-4 text-sm text-white/60">
              兩邊一樣貴（或都是 $0）。
            </div>
          )}
          <div className="grid gap-4 md:grid-cols-2">
            <SideCard
              title="A"
              quote={result.a}
              pending={false}
              winner={!!saving && saving.cheap === "A"}
              modelLabel={result.aLabel}
            />
            <SideCard
              title="B"
              quote={result.b}
              pending={false}
              winner={!!saving && saving.cheap === "B"}
              modelLabel={result.bLabel}
            />
          </div>
        </div>
      )}
    </div>
  );
}
