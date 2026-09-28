import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api, type PriceQuote, type UsableModel } from "../lib/api";
import { PopSelect } from "./PopSelect";
import { type CustomRange } from "./RangePicker";
import { Segmented } from "./Segmented";
import { PriceExtractCard } from "./calc/PriceExtractCard";
import { ResultBlock, type Saving } from "./calc/ResultBlock";
import { ScenarioCard } from "./calc/ScenarioCard";
import {
  fillFromSummary,
  manualQuote,
  manualSubQuote,
  SCENARIOS,
  type ManualRates,
} from "./calc/calcMath";

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
  const saving = useMemo<Saving | null>(() => {
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

      <ScenarioCard
        scenario={scenario}
        onPickScenario={pickScenario}
        dateRange={dateRange}
        onPickDates={pickDates}
        toks={toks}
        setToks={setToks}
        num={num}
      />

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
        <ResultBlock
          a={result.a}
          b={result.b}
          aLabel={result.aLabel}
          bLabel={result.bLabel}
          saving={saving}
        />
      )}
    </div>
  );
}
