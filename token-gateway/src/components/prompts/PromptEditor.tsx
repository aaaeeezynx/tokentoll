/**
 * 提示詞編輯器（P3.2）。
 *
 * cc-switch 的編輯器有語法高亮與即時預覽。我們用純 textarea ＋ 一個**簡易預覽**
 * （標題／清單／程式碼區塊／粗體），不引入 Markdown 引擎 —— 這個 App 的其他輸入
 * 也都是純文字，預覽只是輔助確認排版。
 */
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { api, type PromptInput, type PromptPreset } from "../../lib/api";
import { Icon } from "../icons";

/** 極簡 Markdown → HTML（只處理最常見的幾種；不追求完整規格）。 */
export function miniMarkdown(src: string): string {
  const esc = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const lines = esc(src).split("\n");
  const out: string[] = [];
  let inCode = false;
  let inList = false;
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (line.startsWith("```")) {
      if (inList) {
        out.push("</ul>");
        inList = false;
      }
      out.push(inCode ? "</code></pre>" : "<pre><code>");
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push(line);
      continue;
    }
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      if (inList) {
        out.push("</ul>");
        inList = false;
      }
      const lvl = h[1].length;
      out.push(`<h${lvl}>${h[2]}</h${lvl}>`);
      continue;
    }
    const li = /^[-*]\s+(.*)$/.exec(line);
    if (li) {
      if (!inList) {
        out.push("<ul>");
        inList = true;
      }
      out.push(`<li>${li[1]}</li>`);
      continue;
    }
    if (inList && line.trim() === "") {
      out.push("</ul>");
      inList = false;
      continue;
    }
    if (line.trim() === "") {
      out.push("");
      continue;
    }
    out.push(`<p>${line}</p>`);
  }
  if (inList) out.push("</ul>");
  if (inCode) out.push("</code></pre>");
  return out
    .join("\n")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>");
}

export function PromptEditor(props: {
  app: string;
  initial: PromptPreset | null;
  onClose: () => void;
  onSaved: (p: PromptPreset) => void;
}) {
  const [name, setName] = useState(props.initial?.name ?? "");
  const [content, setContent] = useState(props.initial?.content ?? "");
  const [preview, setPreview] = useState(false);
  const [err, setErr] = useState("");

  const save = useMutation({
    mutationFn: (input: PromptInput) => api.promptSave(input),
    onSuccess: (saved) => props.onSaved(saved),
    onError: (e) => setErr(String(e)),
  });

  const active = props.initial?.active ?? false;
  return (
    <div
      className="fixed inset-0 z-[90] flex justify-center overflow-y-auto p-4 glass-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div className="my-auto flex max-h-[min(92vh,820px)] w-full max-w-2xl flex-col overflow-hidden glass-panel pop-in">
        <div className="flex shrink-0 items-center gap-3 border-b border-fg/[0.06] bg-[var(--app-elevated-solid)] px-5 py-4">
          <button
            className="rounded-full bg-fg/[0.06] p-1.5 text-fg/55 transition-colors hover:bg-fg/[0.1] hover:text-fg"
            onClick={props.onClose}
            title="關閉"
          >
            <Icon name="x" size={14} />
          </button>
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-fg/[0.06] text-fg/65">
            <Icon name="pencil" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold text-fg">
            {props.initial ? `編輯「${props.initial.name}」` : "新增提示詞預設集"}
          </div>
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto px-5 py-5">
          <div className="mac-frow">
            <span className="mac-cap">名稱</span>
            <input
              className="field w-full px-2.5 py-1.5 text-[13px]"
              placeholder="例如：嚴格審查模式"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className="flex items-center gap-2">
            <span className="text-[11px] font-semibold tracking-wider text-fg/40 uppercase">
              內容（Markdown）
            </span>
            <span className="flex-1" />
            <button
              className="btn-ghost px-2.5 py-1 text-[11px]"
              onClick={() => setPreview((v) => !v)}
            >
              {preview ? "回編輯" : "預覽"}
            </button>
          </div>

          {preview ? (
            <div
              className="prose-mini min-h-[46vh] overflow-auto rounded-lg bg-black/25 p-3.5 text-[13px] leading-relaxed text-fg/75"
              // 內容由使用者自己輸入，且已做 HTML 轉義（`miniMarkdown` 的 esc）
              dangerouslySetInnerHTML={{ __html: miniMarkdown(content) }}
            />
          ) : (
            <textarea
              className="field h-[46vh] w-full px-3 py-2.5 font-mono text-[12px] leading-relaxed"
              placeholder={"# 角色\n你是…\n\n## 回應風格\n- 簡潔\n- 給具體例子"}
              value={content}
              onChange={(e) => setContent(e.target.value)}
            />
          )}

          {active && (
            <p className="text-[11px] text-amber-300/80">
              這是**啟用中**的預設集：存檔後會立刻寫進工具檔案。
            </p>
          )}
          {err && (
            <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs break-words text-red-400">
              {err}
            </div>
          )}
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-fg/[0.06] px-5 py-3">
          <button className="btn-ghost px-5 py-1.5 text-[13px]" onClick={props.onClose}>
            取消
          </button>
          <button
            className="btn-primary px-6 py-1.5 text-[13px] disabled:opacity-40"
            disabled={save.isPending}
            onClick={() =>
              save.mutate({ id: props.initial?.id ?? null, app: props.app, name, content })
            }
          >
            {save.isPending ? "儲存中…" : "儲存"}
          </button>
        </div>
      </div>
    </div>
  );
}
