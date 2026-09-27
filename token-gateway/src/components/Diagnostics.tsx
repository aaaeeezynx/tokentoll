import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type TraceRow } from "../lib/api";
import { Icon } from "./icons";
import { Segmented } from "./Segmented";
import { useConfirm } from "./Confirm";
import { appLabel } from "./logos";

/**
 * 診斷中心（Phase 1 可觀測性）。
 *
 * 背景：網關原本對「上游 400」的處理完全不可觀測 —— 剝離了哪些欄位、上游原文
 * 是什麼，只在錯誤當下截斷 300 字透出，事後無法稽核；body 解析失敗更是連根因
 * 都無法判定（見 docs/REFACTORING-PLAN.md §5.2）。本頁把 proxy.rs 被動寫入的
 * `proxy_trace` 與上游能力記憶攤開，讓問題從「只能靠猜」變成「可查」。
 */

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function fmtBytes(n: number | null): string {
  if (n === null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

/** 統計卡。 */
function Stat(props: {
  label: string;
  value: number | string;
  tone?: "warn" | "ok" | "muted";
  hint?: string;
}) {
  const color =
    props.tone === "warn"
      ? "#ff6961"
      : props.tone === "ok"
        ? "#30d158"
        : "var(--mac-label-2)";
  return (
    <div className="glass px-3.5 py-3">
      <div className="text-[11px] text-white/40">{props.label}</div>
      <div
        className="pt-0.5 font-mono text-[19px] leading-tight font-semibold"
        style={{ color }}
      >
        {props.value}
      </div>
      {props.hint && (
        <div className="pt-1 text-[10px] leading-tight text-white/25">
          {props.hint}
        </div>
      )}
    </div>
  );
}

/** 上游狀態碼徽章。 */
function StatusBadge({ status }: { status: number }) {
  if (!status) {
    return <span className="font-mono text-[11px] text-white/30">—</span>;
  }
  const c = status < 400 ? "#30d158" : status < 500 ? "#ffd60a" : "#ff6961";
  return (
    <span
      className="rounded px-1.5 py-px font-mono text-[11px] font-semibold"
      style={{ background: `${c}22`, color: c }}
    >
      {status}
    </span>
  );
}

function SectionHead(props: {
  icon: "alert" | "bolt-fill" | "clock" | "sliders";
  tile: string;
  title: string;
  caption?: string;
  right?: ReactNode;
}) {
  return (
    <div className="mb-3 flex items-center gap-2.5">
      <span
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-white"
        style={{ background: props.tile }}
      >
        <Icon name={props.icon} size={17} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[15px] font-semibold tracking-tight text-white">
          {props.title}
        </div>
        {props.caption && (
          <div className="truncate text-[11px] text-white/30">
            {props.caption}
          </div>
        )}
      </div>
      {props.right}
    </div>
  );
}

/** 單筆追蹤的展開明細。 */
function TraceDetail({ row }: { row: TraceRow }) {
  const kv: [string, ReactNode][] = [
    ["追蹤 ID", <span className="font-mono">{row.id}</span>],
    ["時間", fmtTime(row.ts)],
    ["應用", row.app ? appLabel(row.app) : "—"],
    ["模型（原始）", <span className="font-mono">{row.model_raw || "—"}</span>],
    [
      "轉換",
      // 網關自己拒絕的請求（`trans_kind = "rejected"`）沒有格式轉換、也沒有
      // 上游，`in_fmt`／`target_fmt` 皆為空。若照其他種類的寫法會顯示成
      // 「 → （rejected）」，故這一類單獨呈現。
      row.trans_kind === "rejected"
        ? "網關拒絕（未觸及上游）"
        : `${row.in_fmt} → ${row.target_fmt}（${row.trans_kind}）`,
    ],
    ["延遲", `${row.latency_ms} ms`],
    ["重試次數", String(row.retry_count)],
    [
      "請求體",
      `${fmtBytes(row.content_length)}${row.content_type ? ` · ${row.content_type}` : ""}`,
    ],
    [
      "body SHA-256",
      <span className="font-mono text-[10px] break-all">
        {row.body_sha256 || "—"}
      </span>,
    ],
  ];
  return (
    <div className="border-t border-white/[0.06] px-3.5 py-3">
      <dl className="grid grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
        {kv.map(([k, v]) => (
          <div key={k} className="flex gap-2 text-[12px]">
            <dt className="w-24 shrink-0 text-white/35">{k}</dt>
            <dd className="min-w-0 break-words text-white/70">{v}</dd>
          </div>
        ))}
      </dl>

      {row.stripped_fields.length > 0 && (
        <div className="pt-2.5">
          <div className="text-[11px] text-white/35">已剝離欄位</div>
          <div className="flex flex-wrap gap-1.5 pt-1">
            {row.stripped_fields.map((f) => (
              <span
                key={f}
                className="rounded px-1.5 py-px font-mono text-[11px]"
                style={{
                  background: "rgba(255,214,10,0.14)",
                  color: "#ffd60a",
                }}
              >
                {f}
              </span>
            ))}
          </div>
        </div>
      )}

      {row.upstream_error && (
        <div className="pt-2.5">
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-white/35">上游原文</span>
            <button
              className="btn-ghost flex items-center gap-1 px-2 py-px text-[10px]"
              onClick={() => copyText(row.upstream_error)}
            >
              <Icon name="copy" size={10} />
              複製
            </button>
          </div>
          <pre className="mt-1 max-h-56 overflow-auto rounded-md bg-black/30 p-2.5 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-white/60">
            {row.upstream_error}
          </pre>
        </div>
      )}

      {row.body_hex && (
        <div className="pt-2.5">
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-white/35">
              請求體原始 hex 前綴（body 解析失敗時的關鍵證據）
            </span>
            <button
              className="btn-ghost flex items-center gap-1 px-2 py-px text-[10px]"
              onClick={() => copyText(row.body_hex)}
            >
              <Icon name="copy" size={10} />
              複製
            </button>
          </div>
          <pre className="mt-1 max-h-40 overflow-auto rounded-md bg-black/30 p-2.5 font-mono text-[10px] leading-relaxed break-all whitespace-pre-wrap text-white/55">
            {row.body_hex}
          </pre>
          <p className="pt-1 text-[10px] text-white/25">
            前 {row.body_hex.length / 2} bytes。可由此判定請求體究竟是 XML/HTML
            錯誤頁、空 body，還是合法 JSON 被誤判。
          </p>
        </div>
      )}

      {row.note && (
        <p className="pt-2.5 text-[11px] text-white/45">說明：{row.note}</p>
      )}
    </div>
  );
}

export default function DiagnosticsPage() {
  const qc = useQueryClient();
  const { dialog, ask } = useConfirm();
  const [only, setOnly] = useState("problems");
  const [expanded, setExpanded] = useState<number | null>(null);
  const [msg, setMsg] = useState("");

  const summary = useQuery({
    queryKey: ["trace_summary"],
    queryFn: api.traceSummary,
    refetchInterval: 10000,
  });
  const rows = useQuery({
    queryKey: ["trace_rows", only],
    queryFn: () =>
      only === "problems" ? api.traceProblems(300) : api.traceList(300),
    refetchInterval: 10000,
  });
  const stripped = useQuery({
    queryKey: ["provider_stripped_all"],
    queryFn: api.providerStrippedAll,
    refetchInterval: 10000,
  });

  const clear = useMutation({
    mutationFn: api.traceClear,
    onSuccess: (n) => {
      setMsg(`已清空 ${n} 筆追蹤`);
      void qc.invalidateQueries({ queryKey: ["trace_summary"] });
      void qc.invalidateQueries({ queryKey: ["trace_rows"] });
    },
    onError: (e) => setMsg(`清空失敗：${String(e)}`),
  });

  const resetOne = useMutation({
    mutationFn: (id: number) => api.providerStrippedClear(id),
    onSuccess: (n) => {
      setMsg(`已重設 ${n} 個欄位記憶（下次請求會重新探測該渠道）`);
      void qc.invalidateQueries({ queryKey: ["trace_summary"] });
      void qc.invalidateQueries({ queryKey: ["provider_stripped_all"] });
    },
    onError: (e) => setMsg(`重設失敗：${String(e)}`),
  });

  const s = summary.data;
  const list = rows.data ?? [];

  return (
    <div className="space-y-4">
      {dialog}

      {/* ── 總覽 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="bolt-fill"
          tile="linear-gradient(160deg, #ff9f0a, #c26a00)"
          title="網關診斷"
          caption="異常請求追蹤（網關拒絕、剝離重試、4xx／5xx、解析失敗時寫入）"
          right={
            <button
              className="btn-ghost flex shrink-0 items-center gap-1.5 px-3 py-1.5 text-xs"
              onClick={() => {
                void qc.invalidateQueries({ queryKey: ["trace_summary"] });
                void qc.invalidateQueries({ queryKey: ["trace_rows"] });
                void qc.invalidateQueries({
                  queryKey: ["provider_stripped_all"],
                });
              }}
            >
              <Icon name="refresh" size={12} />
              重新整理
            </button>
          }
        />
        {summary.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : summary.isError ? (
          <p className="text-sm text-white/50">
            讀取失敗：{String(summary.error)}
          </p>
        ) : (
          <>
            <div className="grid grid-cols-2 gap-2.5 md:grid-cols-5">
              <Stat label="追蹤總數" value={s?.total ?? 0} />
              <Stat
                label="異常"
                value={s?.warn_count ?? 0}
                tone={(s?.warn_count ?? 0) > 0 ? "warn" : undefined}
                hint="warn 層級"
              />
              <Stat label="近 24 小時" value={s?.last_24h ?? 0} />
              <Stat
                label="剝離重試"
                value={s?.retried ?? 0}
                tone="muted"
                hint="遇到 400 後剝離重發"
              />
              <Stat
                label="body 解析失敗"
                value={s?.with_body_hex ?? 0}
                tone={(s?.with_body_hex ?? 0) > 0 ? "warn" : "ok"}
                hint="已保留原始 hex"
              />
            </div>

            {/* 上游狀態碼分佈 */}
            {(s?.top_status.length ?? 0) > 0 && (
              <div className="pt-3.5">
                <div className="pb-1.5 text-[11px] text-white/35">
                  上游狀態碼分佈
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {s?.top_status.map((t) => (
                    <span
                      key={t.status}
                      className="flex items-center gap-1.5 rounded-md bg-white/[0.05] px-2.5 py-1"
                    >
                      <StatusBadge status={t.status} />
                      <span className="font-mono text-[11px] text-white/55">
                        {t.count}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )}

            <p className="pt-3 text-[11px] leading-relaxed text-white/25">
              追蹤只記錄異常請求，正常請求不寫入以免日誌洪水；網關啟動時保留最近
              5000 筆。金鑰永不落庫，請求體只存長度與 SHA-256 指紋（僅解析失敗時
              額外存前 512 bytes 的 hex 以供定案）。
            </p>
          </>
        )}
      </div>

      {/* ── 上游拒收欄位記憶 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="sliders"
          tile="linear-gradient(160deg, #ffd60a, #b08a00)"
          title="上游能力記憶"
          caption="網關學到「這個渠道不收這個欄位」後，後續請求會預先剝離"
        />
        {stripped.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : (stripped.data?.length ?? 0) === 0 ? (
          <p className="text-[13px] text-white/35">
            目前沒有記憶 —— 表示尚未遇到上游拒收欄位，或已全部重設。
          </p>
        ) : (
          <div className="space-y-1.5">
            {stripped.data?.map((p) => (
              <div
                key={p.provider_id}
                className="flex flex-wrap items-center gap-2 rounded-md bg-white/[0.04] px-3 py-2"
              >
                <span className="min-w-0 truncate text-[13px] text-white/80">
                  {p.provider_name || (
                    <span className="text-white/35">
                      已刪除的渠道 #{p.provider_id}
                    </span>
                  )}
                </span>
                <span className="font-mono text-[10px] text-white/25">
                  #{p.provider_id}
                </span>
                <span className="flex flex-1 flex-wrap gap-1.5">
                  {p.fields.map((f) => (
                    <span
                      key={f}
                      className="rounded px-1.5 py-px font-mono text-[11px]"
                      style={{
                        background: "rgba(255,214,10,0.14)",
                        color: "#ffd60a",
                      }}
                    >
                      {f}
                    </span>
                  ))}
                </span>
                <button
                  className="btn-ghost shrink-0 px-2.5 py-1 text-[11px]"
                  disabled={resetOne.isPending}
                  onClick={() => {
                    const nm = p.provider_name || `#${p.provider_id}`;
                    ask(
                      "重設此渠道的能力記憶？",
                      () => resetOne.mutate(p.provider_id),
                      {
                        message: `將清除「${nm}」的 ${p.fields.length} 個欄位記憶。下次請求會重新對上游試探，可能會再遇到一次 400 後才重新學會。`,
                        confirmLabel: "重設",
                      },
                    );
                  }}
                >
                  重設
                </button>
              </div>
            ))}
          </div>
        )}
        <p className="pt-3 text-[11px] leading-relaxed text-white/25">
          這份記憶持久化於 SQLite，網關重啟不會遺失 —— 原實作只存在進程記憶體，
          每次重啟都要為每個渠道重踩一次 400。
        </p>
      </div>

      {/* ── 追蹤明細 ── */}
      <div className="glass p-5">
        <SectionHead
          icon="clock"
          tile="linear-gradient(160deg, #3395ff, #0a64d8)"
          title="請求追蹤"
          caption="點任一列展開完整上下文"
          right={
            <div className="flex shrink-0 items-center gap-2">
              <Segmented
                options={[
                  { id: "problems", label: "只看異常" },
                  { id: "all", label: "全部" },
                ]}
                value={only}
                onChange={(v) => {
                  setOnly(v);
                  setExpanded(null);
                }}
                btnClassName="px-2.5 py-1 text-[11px] font-medium"
              />
              <button
                className="btn-ghost flex items-center gap-1.5 px-3 py-1.5 text-xs"
                disabled={clear.isPending || (s?.total ?? 0) === 0}
                onClick={() => {
                  const n = s?.total ?? 0;
                  ask("清空所有追蹤？", () => clear.mutate(), {
                    message: `將刪除全部 ${n} 筆追蹤記錄。此操作不影響用量統計與帳務資料。`,
                    confirmLabel: "清空",
                  });
                }}
              >
                <Icon name="trash" size={12} />
                清空
              </button>
            </div>
          }
        />

        {msg && <p className="pb-2.5 text-xs text-white/50">{msg}</p>}

        {rows.isPending ? (
          <p className="text-sm text-white/30">載入中…</p>
        ) : rows.isError ? (
          <p className="text-sm text-white/50">
            讀取失敗：{String(rows.error)}
          </p>
        ) : list.length === 0 ? (
          <p className="text-[13px] text-white/35">
            {only === "problems"
              ? "沒有異常請求 —— 網關運作正常。"
              : "尚無追蹤記錄。追蹤只在異常時寫入，正常請求不會產生。"}
          </p>
        ) : (
          <div className="overflow-hidden rounded-md border border-white/[0.06]">
            {/* 表頭 */}
            <div
              className="hidden items-center gap-2 px-3 py-1.5 text-[10px] text-white/30 sm:flex"
              style={{ background: "rgba(255,255,255,0.03)" }}
            >
              <span className="w-32 shrink-0">時間</span>
              <span className="w-14 shrink-0">應用</span>
              <span className="w-16 shrink-0">狀態</span>
              <span className="min-w-0 flex-1">模型 / 轉換</span>
              <span className="w-28 shrink-0">剝離欄位</span>
              <span className="w-16 shrink-0 text-right">延遲</span>
            </div>
            {list.map((r) => (
              <div key={r.id} className="border-t border-white/[0.05]">
                <button
                  className="flex w-full flex-wrap items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-white/[0.03]"
                  onClick={() => setExpanded(expanded === r.id ? null : r.id)}
                >
                  <span className="w-32 shrink-0 font-mono text-[11px] text-white/45">
                    {fmtTime(r.ts).slice(5)}
                  </span>
                  <span className="w-14 shrink-0 truncate text-[12px] text-white/70">
                    {r.app || "—"}
                  </span>
                  <span className="flex w-16 shrink-0 items-center gap-1">
                    <StatusBadge status={r.upstream_status} />
                  </span>
                  <span className="min-w-0 flex-1 truncate text-[12px] text-white/60">
                    <span className="font-mono">{r.model_raw || "—"}</span>
                    <span className="px-1.5 text-white/25">·</span>
                    <span className="text-[11px] text-white/35">
                      {r.in_fmt} → {r.target_fmt}
                    </span>
                    {r.level === "warn" && (
                      <span
                        className="ml-2 rounded px-1.5 py-px text-[10px]"
                        style={{
                          background: "rgba(255,69,58,0.14)",
                          color: "#ff6961",
                        }}
                      >
                        異常
                      </span>
                    )}
                  </span>
                  <span className="w-28 shrink-0 truncate font-mono text-[11px] text-white/45">
                    {r.stripped_fields.length > 0
                      ? r.stripped_fields.join(", ")
                      : r.retry_count > 0
                        ? `重試 ${r.retry_count} 次`
                        : "—"}
                  </span>
                  <span className="w-16 shrink-0 text-right font-mono text-[11px] text-white/40">
                    {r.latency_ms}ms
                  </span>
                  <Icon
                    name={expanded === r.id ? "chevron-down" : "chevron-right"}
                    size={12}
                    className="shrink-0 text-white/25"
                  />
                </button>
                {expanded === r.id && <TraceDetail row={r} />}
              </div>
            ))}
            {list.length >= 300 && (
              <div className="border-t border-white/[0.05] px-3 py-1.5 text-[10px] text-white/25">
                僅顯示最近 300 筆。清空或調整範圍以查看更新記錄。
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
