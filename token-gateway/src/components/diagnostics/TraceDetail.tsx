//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

import type { ReactNode } from "react";
import { type TraceRow } from "../../lib/api";
import { Icon } from "../icons";
import { appLabel } from "../logos";
import { copyText, fmtBytes, fmtTime } from "./shared";

/** 單筆追蹤的展開明細。 */
export function TraceDetail({ row }: { row: TraceRow }) {
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
    <div className="border-t border-fg/[0.06] px-3.5 py-3">
      <dl className="grid grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
        {kv.map(([k, v]) => (
          <div key={k} className="flex gap-2 text-[12px]">
            <dt className="w-24 shrink-0 text-fg/35">{k}</dt>
            <dd className="min-w-0 break-words text-fg/70">{v}</dd>
          </div>
        ))}
      </dl>

      {row.stripped_fields.length > 0 && (
        <div className="pt-2.5">
          <div className="text-[11px] text-fg/35">已剝離欄位</div>
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
            <span className="text-[11px] text-fg/35">上游原文</span>
            <button
              className="btn-ghost flex items-center gap-1 px-2 py-px text-[10px]"
              onClick={() => copyText(row.upstream_error)}
            >
              <Icon name="copy" size={10} />
              複製
            </button>
          </div>
          <pre className="mt-1 max-h-56 overflow-auto rounded-md bg-black/30 p-2.5 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-fg/60">
            {row.upstream_error}
          </pre>
        </div>
      )}

      {row.body_hex && (
        <div className="pt-2.5">
          <div className="flex items-center gap-1.5">
            <span className="text-[11px] text-fg/35">
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
          <pre className="mt-1 max-h-40 overflow-auto rounded-md bg-black/30 p-2.5 font-mono text-[10px] leading-relaxed break-all whitespace-pre-wrap text-fg/55">
            {row.body_hex}
          </pre>
          <p className="pt-1 text-[10px] text-fg/25">
            前 {row.body_hex.length / 2} bytes。可由此判定請求體究竟是 XML/HTML
            錯誤頁、空 body，還是合法 JSON 被誤判。
          </p>
        </div>
      )}

      {row.note && (
        <p className="pt-2.5 text-[11px] text-fg/45">說明：{row.note}</p>
      )}
    </div>
  );
}
