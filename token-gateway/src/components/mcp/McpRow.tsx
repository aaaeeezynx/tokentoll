/**
 * MCP 列表中的一列：伺服器資訊 ＋ 逐工具開關 ＋ 編輯／刪除。
 *
 * 純呈現（開關與動作都由 `Mcp.tsx` 傳進來），與來源卡片同一個設計語言。
 */
import type { McpServer } from "../../lib/api";
import { Icon } from "../icons";
import { Toggle } from "../Toggle";

export function McpRow(props: {
  s: McpServer;
  apps: string[];
  appLabel: (app: string) => string;
  onToggle: (app: string, enabled: boolean) => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const { s } = props;
  const on = (app: string) =>
    s.bindings.find((b) => b.app === app)?.enabled ?? false;
  const detail =
    s.transport === "stdio"
      ? `${s.command} ${s.args.join(" ")}`.trim()
      : s.url;
  return (
    <div className="rounded-xl border border-fg/[0.06] p-3.5 transition-colors hover:border-fg/10">
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1">
          <span className="truncate text-sm font-semibold text-fg/85">
            {s.name || s.slug}
          </span>
          <span className="ml-2 rounded bg-fg/[0.06] px-1.5 py-px font-mono text-[10px] text-fg/40">
            {s.transport}
          </span>
          <span className="ml-1.5 font-mono text-[10px] text-fg/25">{s.slug}</span>
        </span>
        {props.apps.map((app) => (
          <span
            key={app}
            className="flex w-[92px] shrink-0 justify-center"
            title={`${props.appLabel(app)}：${on(app) ? "已啟用" : "未啟用"}`}
          >
            <Toggle
              checked={on(app)}
              onChange={(v) => props.onToggle(app, v)}
              size="sm"
            />
          </span>
        ))}
        <span className="flex w-[52px] shrink-0 justify-end gap-1">
          <button
            className="rounded-full p-1.5 text-fg/45 transition-colors hover:bg-fg/10 hover:text-fg"
            title="編輯"
            onClick={props.onEdit}
          >
            <Icon name="pencil" size={13} />
          </button>
          <button
            className="rounded-full p-1.5 text-fg/45 transition-colors hover:bg-red-500/10 hover:text-red-400"
            title="刪除"
            onClick={props.onDelete}
          >
            <Icon name="trash" size={13} />
          </button>
        </span>
      </div>
      <p className="mt-1 truncate font-mono text-[11px] text-fg/30" title={detail}>
        {detail}
      </p>
      {s.description && (
        <p className="mt-0.5 truncate text-[11px] text-fg/25">{s.description}</p>
      )}
      {(s.env.length > 0 || s.headers.length > 0) && (
        <p className="mt-0.5 truncate font-mono text-[10px] text-fg/20">
          {[...s.env, ...s.headers].map((p) => p.name).join(", ")}
        </p>
      )}
    </div>
  );
}
