/**
 * 用量查詢設定面板（P2.1，對標 cc-switch 的 Usage Query 面板）。
 *
 * 照它的欄位：啟用開關、樣板選擇、API Key／Base URL／Access Token／User ID、
 * 逾時（預設 10 秒）、自動查詢間隔（0–1440 分，0＝關），以及**測試**按鈕
 * （真的打一次、顯示結果或錯誤）。
 *
 * 差別只有一格：自訂查詢存的是**宣告式抽取規格**而不是 JS（見後端
 * `usage_query/extract.rs` 的三個理由）。面板因此把「抽取規格」做成欄位化的
 * JSON 編輯區，並在測試結果裡同時顯示原始回應，讓使用者看得出樣板對不對。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  type QuotaView,
  type UsageQueryConfig,
  type UsageTemplate,
} from "../../lib/api";
import { Icon } from "../icons";
import { Toggle } from "../Toggle";
import { PopSelect } from "../PopSelect";

const fieldCls = "field w-full px-2.5 py-1.5 text-[12px]";

/** 抽取規格（物件）→ 好編輯的文字。 */
function specText(v: unknown): string {
  try {
    return JSON.stringify(v ?? {}, null, 2);
  } catch {
    return "{}";
  }
}

export function UsageQueryPanel(props: {
  providerId: number;
  providerName: string;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [cfg, setCfg] = useState<UsageQueryConfig | null>(null);
  const [spec, setSpec] = useState("{}");
  const [specErr, setSpecErr] = useState("");
  const [msg, setMsg] = useState("");
  const [test, setTest] = useState<QuotaView | null>(null);
  const [showRaw, setShowRaw] = useState(false);

  const cur = useQuery({
    queryKey: ["usage_query", props.providerId],
    queryFn: () => api.usageQueryGet(props.providerId),
  });
  const tpls = useQuery({
    queryKey: ["usage_query_templates"],
    queryFn: api.usageQueryTemplates,
  });

  useEffect(() => {
    if (cur.data) {
      setCfg(cur.data);
      setSpec(specText(cur.data.extractor));
    }
  }, [cur.data]);

  const set = <K extends keyof UsageQueryConfig>(k: K, v: UsageQueryConfig[K]) =>
    setCfg((c) => (c ? { ...c, [k]: v } : c));

  /** 面板上的值 → 要送出的設定（含抽取規格的解析）。 */
  const collect = (): UsageQueryConfig | null => {
    if (!cfg) return null;
    let extractor: unknown;
    try {
      extractor = JSON.parse(spec || "{}");
      setSpecErr("");
    } catch (e) {
      setSpecErr(`抽取規格不是合法 JSON：${String(e)}`);
      return null;
    }
    return { ...cfg, extractor };
  };

  const applyTpl = useMutation({
    mutationFn: (t: UsageTemplate) =>
      api.usageQueryApplyTemplate(props.providerId, t.id),
    onSuccess: (next) => {
      // 套樣板＝「我要用它」→ 順手把開關打開（開關本身就在畫面上，可以再關掉）。
      // 不這樣做的話，套完樣板按儲存卻什麼都不會顯示，使用者只會覺得壞了。
      const on = { ...next, enabled: true };
      setCfg(on);
      setSpec(specText(on.extractor));
      setMsg("已套用樣板並啟用（記得按儲存）");
      setTest(null);
    },
    onError: (e) => setMsg(`套用樣板失敗：${String(e)}`),
  });

  const save = useMutation({
    mutationFn: (c: UsageQueryConfig) => api.usageQuerySet(c),
    onSuccess: (saved) => {
      setCfg(saved);
      setMsg("已儲存");
      void qc.invalidateQueries({ queryKey: ["usage_query", props.providerId] });
      void qc.invalidateQueries({ queryKey: ["usage_quota_all"] });
    },
    onError: (e) => setMsg(`儲存失敗：${String(e)}`),
  });

  const clear = useMutation({
    mutationFn: () => api.usageQueryClear(props.providerId),
    onSuccess: () => {
      setMsg("已清除設定");
      setCfg((c) => (c ? { ...c, enabled: false, url_template: "" } : c));
      void qc.invalidateQueries({ queryKey: ["usage_query", props.providerId] });
      void qc.invalidateQueries({ queryKey: ["usage_quota_all"] });
    },
    onError: (e) => setMsg(`清除失敗：${String(e)}`),
  });

  const runTest = async () => {
    const c = collect();
    if (!c) return;
    setMsg("測試中…");
    setTest(null);
    try {
      // 先存再測：確保「測到的」就是「存起來的」（cc-switch 也是這個順序）
      await api.usageQuerySet(c);
      const view = await api.usageQueryRun(props.providerId);
      setTest(view);
      setMsg(
        view.ok
          ? "測試成功" + (c.enabled ? "" : "（但「啟用查詢」是關的，卡片不會顯示）")
          : "測試失敗（見下方訊息）",
      );
      void qc.invalidateQueries({ queryKey: ["usage_query", props.providerId] });
      void qc.invalidateQueries({ queryKey: ["usage_quota_all"] });
    } catch (e) {
      setMsg(`測試失敗：${String(e)}`);
    }
  };

  const tpl = tpls.data?.find((t) => t.id === cfg?.template) ?? null;
  const needs = tpl?.config.needs ?? [];

  return (
    <div
      className="fixed inset-0 z-[90] flex justify-center overflow-y-auto p-4 glass-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div className="my-auto flex max-h-[min(90vh,780px)] w-full max-w-lg flex-col overflow-hidden glass-panel pop-in">
        <div className="flex shrink-0 items-center gap-3 border-b border-white/[0.06] bg-[rgba(24,24,30,0.98)] px-5 py-4">
          <button
            className="rounded-full bg-white/[0.06] p-1.5 text-white/55 transition-colors hover:bg-white/[0.1] hover:text-white"
            onClick={props.onClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-white/[0.06] text-white/65">
            <Icon name="chart-bar" size={17} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[15px] font-semibold text-white">
              用量查詢 · {props.providerName}
            </div>
            <div className="text-[11px] text-white/30">
              查餘額或方案額度；查詢會消耗少量上游額度，所以自動查詢預設關閉
            </div>
          </div>
        </div>

        <div className="flex-1 space-y-4 overflow-y-auto px-5 py-5">
          {cur.isPending ? (
            <p className="text-sm text-white/30">載入中…</p>
          ) : !cfg ? (
            <p className="text-sm text-white/50">讀取設定失敗</p>
          ) : (
            <>
              <div className="mac-frow">
                <span className="mac-cap">啟用查詢</span>
                <Toggle
                  checked={cfg.enabled}
                  onChange={(v) => set("enabled", v)}
                />
              </div>

              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  樣板
                </div>
                <PopSelect
                  value={cfg.template}
                  onChange={(v) => {
                    const t = tpls.data?.find((x) => x.id === v);
                    if (t) applyTpl.mutate(t);
                    else set("template", v);
                  }}
                  options={(tpls.data ?? []).map((t) => ({
                    value: t.id,
                    label: t.name,
                  }))}
                />
                {tpl && (
                  <p className="pt-1.5 text-[11px] leading-relaxed text-amber-300/80">
                    {tpl.note}
                  </p>
                )}
              </div>

              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  連線
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">查詢網址</span>
                  <input
                    className={fieldCls}
                    placeholder="{{baseUrl}}/user/balance"
                    value={cfg.url_template}
                    onChange={(e) => set("url_template", e.target.value)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">方法</span>
                  <PopSelect
                    value={cfg.method}
                    onChange={(v) => set("method", v)}
                    options={[
                      { value: "GET", label: "GET" },
                      { value: "POST", label: "POST" },
                    ]}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">Base URL</span>
                  <input
                    className={fieldCls}
                    placeholder="留空 = 用來源的"
                    value={cfg.base_url}
                    onChange={(e) => set("base_url", e.target.value)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">API Key</span>
                  <input
                    className={fieldCls}
                    placeholder="留空 = 用來源的"
                    value={cfg.api_key}
                    onChange={(e) => set("api_key", e.target.value)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">Access Token</span>
                  <input
                    className={fieldCls}
                    placeholder={needs.includes("access_token") ? "此樣板必填" : "選填"}
                    value={cfg.access_token}
                    onChange={(e) => set("access_token", e.target.value)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">User ID</span>
                  <input
                    className={fieldCls}
                    placeholder={needs.includes("user_id") ? "此樣板必填" : "選填"}
                    value={cfg.user_id}
                    onChange={(e) => set("user_id", e.target.value)}
                  />
                </div>
                <p className="text-[11px] leading-relaxed text-white/25">
                  網址與標頭可用 <span className="font-mono">{"{{baseUrl}}"}</span>、
                  <span className="font-mono">{"{{apiKey}}"}</span>、
                  <span className="font-mono">{"{{accessToken}}"}</span>、
                  <span className="font-mono">{"{{userId}}"}</span> 佔位符。
                </p>
              </div>

              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  標頭（選填）
                </div>
                {cfg.headers.map((h, i) => (
                  <div key={i} className="mb-1 flex items-center gap-1.5">
                    <input
                      className={fieldCls}
                      placeholder="名稱"
                      value={h.name}
                      onChange={(e) =>
                        set(
                          "headers",
                          cfg.headers.map((x, j) =>
                            j === i ? { ...x, name: e.target.value } : x,
                          ),
                        )
                      }
                    />
                    <input
                      className={fieldCls}
                      placeholder="值"
                      value={h.value}
                      onChange={(e) =>
                        set(
                          "headers",
                          cfg.headers.map((x, j) =>
                            j === i ? { ...x, value: e.target.value } : x,
                          ),
                        )
                      }
                    />
                    <button
                      className="btn-ghost shrink-0 px-2 py-1 text-[11px]"
                      onClick={() =>
                        set("headers", cfg.headers.filter((_, j) => j !== i))
                      }
                    >
                      刪
                    </button>
                  </div>
                ))}
                <button
                  className="btn-ghost flex items-center gap-1 px-2.5 py-1 text-[11px]"
                  onClick={() => set("headers", [...cfg.headers, { name: "", value: "" }])}
                >
                  <Icon name="plus" size={10} />
                  加一個標頭
                </button>
              </div>

              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  抽取規格（怎麼從回應裡取數字）
                </div>
                <textarea
                  className="field h-32 w-full px-2.5 py-2 font-mono text-[11px]"
                  value={spec}
                  onChange={(e) => setSpec(e.target.value)}
                />
                {specErr && <p className="text-[11px] text-red-400">{specErr}</p>}
                <p className="text-[11px] leading-relaxed text-white/25">
                  路徑用點分隔、數字段是陣列索引（例如
                  <span className="font-mono"> balance_infos.0.total_balance</span>）；
                  要換算就寫成物件（例如
                  <span className="font-mono">{`{"path":"quota","divide":500000}`}</span>）。
                  多方案用 <span className="font-mono">plans</span>（見 New API 樣板）。
                </p>
              </div>

              <div className="form-section">
                <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                  進階
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">逾時（秒）</span>
                  <input
                    className={fieldCls}
                    type="number"
                    min={1}
                    max={120}
                    value={cfg.timeout_secs}
                    onChange={(e) => set("timeout_secs", Number(e.target.value) || 10)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">自動查詢（分）</span>
                  <input
                    className={fieldCls}
                    type="number"
                    min={0}
                    max={1440}
                    value={cfg.auto_interval_min}
                    onChange={(e) =>
                      set("auto_interval_min", Number(e.target.value) || 0)
                    }
                  />
                </div>
                <p className="text-[11px] leading-relaxed text-white/25">
                  自動查詢 <span className="font-mono">0</span> ＝ 不自動查（預設）。
                  cc-switch 只對「目前使用中」的來源自動查；我們也只在這個來源被選取時
                  才按間隔重查，避免打擾上游。
                </p>
              </div>

              {test && (
                <div className="rounded-lg bg-white/[0.04] px-3 py-2.5 text-[12px]">
                  <div className="flex items-center gap-2">
                    <span
                      className="rounded px-1.5 py-px text-[11px]"
                      style={
                        test.ok
                          ? { background: "rgba(48,209,88,0.14)", color: "#30d158" }
                          : { background: "rgba(255,69,58,0.16)", color: "#ff6961" }
                      }
                    >
                      {test.ok ? "成功" : "失敗"}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-white/70">
                      {test.ok
                        ? `${test.plan_name || "餘額"}：${test.remaining ?? "—"} ${test.unit}${
                            test.percent !== null ? `（用量 ${Math.round(test.percent)}%）` : ""
                          }`
                        : test.message}
                    </span>
                    <button
                      className="btn-ghost shrink-0 px-2 py-0.5 text-[11px]"
                      onClick={() => setShowRaw((v) => !v)}
                    >
                      {showRaw ? "收起回應" : "看回應"}
                    </button>
                  </div>
                  {showRaw && (
                    <pre className="mt-1.5 max-h-40 overflow-auto font-mono text-[10px] break-all whitespace-pre-wrap text-white/45">
                      {test.raw || "（空）"}
                    </pre>
                  )}
                </div>
              )}

              {msg && <p className="text-xs text-white/50">{msg}</p>}
            </>
          )}
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-white/[0.06] px-5 py-3">
          <button
            className="btn-ghost px-3 py-1.5 text-xs disabled:opacity-40"
            disabled={clear.isPending}
            onClick={() => clear.mutate()}
          >
            清除設定
          </button>
          <button
            className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs disabled:opacity-40"
            disabled={!cfg}
            onClick={() => void runTest()}
          >
            <Icon name="bolt-fill" size={11} />
            測試
          </button>
          <button
            className="btn-primary px-4 py-1.5 text-xs disabled:opacity-40"
            disabled={!cfg || save.isPending}
            onClick={() => {
              const c = collect();
              if (c) save.mutate(c);
            }}
          >
            {save.isPending ? "儲存中…" : "儲存"}
          </button>
        </div>
      </div>
    </div>
  );
}
