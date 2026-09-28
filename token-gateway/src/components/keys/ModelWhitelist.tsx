import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { Icon } from "../icons";

/** 模型白名單：抓取目錄／映射勾選＋手動輸入。 */
export function ModelWhitelist(props: {
  providerId: string;
  value: string[];
  onChange: (v: string[]) => void;
}) {
  const qc = useQueryClient();
  const pid = props.providerId === "" ? null : Number(props.providerId);
  const catalog = useQuery({
    queryKey: ["catalog", pid],
    queryFn: () => api.catalogGet(pid as number),
    retry: false,
    enabled: pid != null,
  });
  const umodels = useQuery({
    queryKey: ["umodels", pid],
    queryFn: () => api.modelsList(pid as number),
    enabled: pid != null,
  });
  const [manual, setManual] = useState("");
  const [filter, setFilter] = useState("");
  const [msg, setMsg] = useState("");
  const fetch = useMutation({
    mutationFn: () => api.catalogFetch(pid as number),
    onSuccess: () => {
      setMsg("");
      void qc.invalidateQueries({ queryKey: ["catalog", pid] });
    },
    onError: (e) => setMsg(String(e)),
  });

  const avail = useMemo(() => {
    const map = new Map<string, string>();
    for (const m of catalog.data?.models || []) map.set(m.id, "目錄");
    for (const m of umodels.data || []) {
      if (!map.has(m.actual_model)) map.set(m.actual_model, "映射");
    }
    for (const v of props.value) {
      if (!map.has(v)) map.set(v, "已選");
    }
    return [...map.entries()].sort(([a], [b]) => (a < b ? -1 : 1));
  }, [catalog.data, umodels.data, props.value]);

  const toggle = (id: string) =>
    props.onChange(
      props.value.includes(id)
        ? props.value.filter((x) => x !== id)
        : [...props.value, id],
    );

  const filtered = filter.trim()
    ? avail.filter(
        ([id]) =>
          id.toLowerCase().includes(filter.trim().toLowerCase()) ||
          props.value.includes(id),
      )
    : avail;

  const addManual = () => {
    const v = manual.trim();
    if (v && !props.value.includes(v)) {
      props.onChange([...props.value, v]);
      setManual("");
    }
  };

  return (
    <div className="space-y-2">
      {props.value.length > 0 && (
        <>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-white/30">
            已選 {props.value.length} 個（空＝全部允許）
          </span>
          <span className="flex-1" />
          <button
            type="button"
            className="shrink-0 rounded-full px-2 py-0.5 text-[11px] text-white/35 transition-colors hover:bg-white/[0.07] hover:text-white/70"
            onClick={() => props.onChange([])}
          >
            清空
          </button>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {props.value.map((v) => (
            <button
              key={v}
              type="button"
              title="移除"
              onClick={() => toggle(v)}
              className="flex max-w-full items-center gap-1.5 rounded-full bg-white/[0.07] px-2.5 py-[3px] font-mono text-[11px] text-white/70 transition-colors hover:bg-red-500/15 hover:text-red-300"
            >
              <span className="truncate">{v}</span>
              <Icon name="x" size={10} className="shrink-0 text-white/40" />
            </button>
          ))}
        </div>
        </>
      )}

      {pid == null ? (
        <p className="text-[11px] text-white/30">
          先選上游來源，再從清單勾選；或手動輸入模型 ID。
        </p>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1 text-[11px] disabled:opacity-40"
              disabled={fetch.isPending}
              onClick={() => fetch.mutate()}
            >
              <Icon name="download" size={11} />
              {fetch.isPending ? "抓取中…" : "抓取目錄"}
            </button>
            <span className="truncate text-[11px] text-white/30">
              {catalog.data
                ? `${catalog.data.models.length} 個模型`
                : "尚未抓取"}
              {(umodels.data?.length ?? 0) > 0 &&
                ` · ${umodels.data?.length ?? 0} 個映射`}
            </span>
          </div>

          {msg && <p className="text-[11px] text-amber-300/80">{msg}</p>}

          {avail.length > 0 && (
            <>
              <div className="relative">
                <Icon
                  name="search"
                  size={12}
                  className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-white/25"
                />
                <input
                  className="field w-full py-1.5 pr-3 pl-7 text-[12px]"
                  placeholder="搜尋模型名稱…"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
              </div>
              <div className="max-h-40 space-y-0.5 overflow-y-auto rounded-lg border border-white/[0.06] bg-black/15 p-1">
                {filtered.length === 0 ? (
                  <p className="px-2 py-3 text-center text-[11px] text-white/25">無匹配</p>
                ) : (
                  filtered.map(([id, src]) => (
                    <label
                      key={id}
                      className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-[12px] text-white/60 transition-colors hover:bg-white/[0.05]"
                    >
                      <input
                        type="checkbox"
                        checked={props.value.includes(id)}
                        onChange={() => toggle(id)}
                        className="accent-[var(--mac-accent)]"
                      />
                      <span className="min-w-0 flex-1 truncate font-mono text-[11px]">{id}</span>
                      <span className="shrink-0 text-[10px] text-white/25">{src}</span>
                    </label>
                  ))
                )}
              </div>
            </>
          )}
        </>
      )}

      <div className="flex gap-1.5">
        <input
          className="field w-full px-2.5 py-1.5 text-[12px]"
          placeholder="手動輸入模型 ID"
          value={manual}
          onChange={(e) => setManual(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") addManual();
          }}
        />
        <button
          type="button"
          className="btn-ghost flex shrink-0 items-center gap-1 px-2.5 py-1.5 text-[12px] disabled:opacity-40"
          disabled={manual.trim() === ""}
          onClick={addManual}
        >
          <Icon name="plus" size={11} />
          新增
        </button>
      </div>
    </div>
  );
}
