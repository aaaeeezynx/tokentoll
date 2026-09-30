/**
 * MCP 伺服器的新增／編輯表單（P3.1）。
 *
 * 欄位與 cc-switch 一致：Server ID（必填、唯一）、名稱、說明、傳輸型別
 * （stdio／http／sse）、Command（stdio 必填）、Arguments、URL（http／sse 必填）、
 * Headers、環境變數。新增時可以先套「樣板」（fetch／time／memory／
 * sequential-thinking／context7）。
 *
 * 存檔後立刻同步（後端 `mcp_upsert` 會把已啟用的工具設定檔一起更新）——
 * 這裡把同步結果回報給呼叫端顯示，失敗時看得到原因。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  api,
  type McpInput,
  type McpPair,
  type McpPreset,
  type McpServer,
  type McpSyncOutcome,
} from "../../lib/api";
import { Icon } from "../icons";
import { PopSelect } from "../PopSelect";

const fieldCls = "field w-full px-2.5 py-1.5 text-[12px]";

const EMPTY: McpInput = {
  slug: "",
  name: "",
  description: "",
  transport: "stdio",
  command: "",
  args: [],
  url: "",
  headers: [],
  env: [],
};

export function McpForm(props: {
  initial: McpServer | null;
  onClose: () => void;
  onSaved: (outcomes: McpSyncOutcome[]) => void;
}) {
  const [form, setForm] = useState<McpInput>(() =>
    props.initial
      ? {
          id: props.initial.id,
          slug: props.initial.slug,
          name: props.initial.name,
          description: props.initial.description,
          transport: props.initial.transport,
          command: props.initial.command,
          args: props.initial.args,
          url: props.initial.url,
          headers: props.initial.headers,
          env: props.initial.env,
        }
      : EMPTY,
  );
  const [argsText, setArgsText] = useState("");
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const presets = useQuery({
    queryKey: ["mcp_presets"],
    queryFn: api.mcpPresets,
    enabled: props.initial === null,
  });

  useEffect(() => {
    setArgsText((props.initial?.args ?? []).join(" "));
  }, [props.initial]);

  const set = <K extends keyof McpInput>(k: K, v: McpInput[K]) =>
    setForm((f) => ({ ...f, [k]: v }));

  const pickPreset = (p: McpPreset) => {
    setForm({
      ...EMPTY,
      slug: p.input.slug,
      name: p.input.name,
      description: p.input.description,
      transport: p.input.transport,
      command: p.input.command,
      args: p.input.args,
      url: p.input.url,
    });
    setArgsText(p.input.args.join(" "));
    setMsg(`已套用樣板「${p.name}」（可再改；記得按儲存）`);
    setErr("");
  };

  const save = useMutation({
    mutationFn: (input: McpInput) => api.mcpUpsert(input),
    onSuccess: async (saved) => {
      // 存檔後同步一次，把結果帶回去（存檔本身已成功，同步失敗只提示）
      try {
        const outcomes = await api.mcpSync();
        props.onSaved(outcomes);
      } catch {
        props.onSaved([]);
      }
      setMsg(`已儲存 ${saved.name || saved.slug}`);
    },
    onError: (e) => setErr(String(e)),
  });

  const submit = () => {
    setErr("");
    const input: McpInput = {
      ...form,
      args: argsText.split(/\s+/).filter(Boolean),
    };
    save.mutate(input);
  };

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
            <Icon name="cpu" size={17} />
          </span>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold text-white">
            {props.initial ? `編輯「${props.initial.name}」` : "新增 MCP 伺服器"}
          </div>
        </div>

        <div className="flex-1 space-y-4 overflow-y-auto px-5 py-5">
          {props.initial === null && (
            <div className="form-section">
              <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                從樣板開始（可選）
              </div>
              <div className="flex flex-wrap gap-1.5">
                {(presets.data ?? []).map((p) => (
                  <button
                    key={p.id}
                    className="rounded-lg border border-white/[0.08] px-2.5 py-1.5 text-left text-[12px] text-white/70 transition-colors hover:border-white/20 hover:bg-white/[0.04]"
                    onClick={() => pickPreset(p)}
                    title={`${p.package}｜${p.note}`}
                  >
                    {p.name}
                  </button>
                ))}
              </div>
              <p className="pt-1.5 text-[11px] leading-relaxed text-white/25">
                樣板的套件名照抄 cc-switch 手冊；啟動指令是我們的判斷
                （Python 伺服器用 uvx、TypeScript 用 npx -y）。沒有那個指令時伺服器
                起不來，記得先安裝。
              </p>
            </div>
          )}

          <div className="form-section">
            <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
              基本
            </div>
            <div className="mac-frow">
              <span className="mac-cap">Server ID</span>
              <input
                className={fieldCls}
                placeholder="例如 context7（設定檔裡的鍵，唯一）"
                value={form.slug}
                onChange={(e) => set("slug", e.target.value)}
              />
            </div>
            <div className="mac-frow">
              <span className="mac-cap">名稱</span>
              <input
                className={fieldCls}
                placeholder="留空＝用 Server ID"
                value={form.name}
                onChange={(e) => set("name", e.target.value)}
              />
            </div>
            <div className="mac-frow">
              <span className="mac-cap">說明</span>
              <input
                className={fieldCls}
                placeholder="選填"
                value={form.description}
                onChange={(e) => set("description", e.target.value)}
              />
            </div>
            <div className="mac-frow">
              <span className="mac-cap">傳輸型別</span>
              <PopSelect
                value={form.transport}
                onChange={(v) => set("transport", v)}
                options={[
                  { value: "stdio", label: "stdio（本機指令）" },
                  { value: "http", label: "http（遠端）" },
                  { value: "sse", label: "sse（遠端推送）" },
                ]}
              />
            </div>
          </div>

          <div className="form-section">
            <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
              {form.transport === "stdio" ? "指令" : "連線"}
            </div>
            {form.transport === "stdio" ? (
              <>
                <div className="mac-frow">
                  <span className="mac-cap">Command</span>
                  <input
                    className={fieldCls}
                    placeholder="例如 npx"
                    value={form.command}
                    onChange={(e) => set("command", e.target.value)}
                  />
                </div>
                <div className="mac-frow">
                  <span className="mac-cap">Arguments</span>
                  <input
                    className={fieldCls}
                    placeholder="以空白分隔，例如 -y @upstash/context7-mcp"
                    value={argsText}
                    onChange={(e) => setArgsText(e.target.value)}
                  />
                </div>
              </>
            ) : (
              <div className="mac-frow">
                <span className="mac-cap">URL</span>
                <input
                  className={fieldCls}
                  placeholder="https://example.com/mcp"
                  value={form.url}
                  onChange={(e) => set("url", e.target.value)}
                />
              </div>
            )}
          </div>

          <div className="form-section">
            <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
              環境變數
            </div>
            <PairEditor
              pairs={form.env}
              onChange={(v) => set("env", v)}
              keyPlaceholder="名稱（例如 API_KEY）"
            />
          </div>

          {form.transport !== "stdio" && (
            <div className="form-section">
              <div className="mb-1 text-[11px] font-semibold tracking-wider text-white/40 uppercase">
                標頭
              </div>
              <PairEditor
                pairs={form.headers}
                onChange={(v) => set("headers", v)}
                keyPlaceholder="名稱（例如 Authorization）"
              />
            </div>
          )}

          {err && (
            <div className="rounded-lg border border-red-500/20 bg-red-500/[0.05] px-3 py-2 text-xs break-words text-red-400">
              {err}
            </div>
          )}
          {msg && <p className="text-xs text-white/50">{msg}</p>}
        </div>

        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-white/[0.06] px-5 py-3">
          <button className="btn-ghost px-5 py-1.5 text-[13px]" onClick={props.onClose}>
            取消
          </button>
          <button
            className="btn-primary px-6 py-1.5 text-[13px] disabled:opacity-40"
            disabled={save.isPending}
            onClick={submit}
          >
            {save.isPending ? "儲存中…" : "儲存"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** 名稱／值配對的編輯器（環境變數與標頭共用）。 */
function PairEditor(props: {
  pairs: McpPair[];
  onChange: (v: McpPair[]) => void;
  keyPlaceholder: string;
}) {
  return (
    <>
      {props.pairs.map((p, i) => (
        <div key={i} className="mb-1 flex items-center gap-1.5">
          <input
            className={fieldCls}
            placeholder={props.keyPlaceholder}
            value={p.name}
            onChange={(e) =>
              props.onChange(
                props.pairs.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)),
              )
            }
          />
          <input
            className={fieldCls}
            placeholder="值"
            value={p.value}
            onChange={(e) =>
              props.onChange(
                props.pairs.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)),
              )
            }
          />
          <button
            className="btn-ghost shrink-0 px-2 py-1 text-[11px]"
            onClick={() => props.onChange(props.pairs.filter((_, j) => j !== i))}
          >
            刪
          </button>
        </div>
      ))}
      <button
        className="btn-ghost flex items-center gap-1 px-2.5 py-1 text-[11px]"
        onClick={() => props.onChange([...props.pairs, { name: "", value: "" }])}
      >
        <Icon name="plus" size={10} />
        加一列
      </button>
    </>
  );
}
