/**
 * 「從預設開始」清單（對齊 CC Switch 的 90+ provider presets）。
 *
 * 預設只帶**連線資訊**（名稱／上游地址／協議／鑑權），刻意不含模型清單：
 * 模型名變動快，一律用表單上的「取得模型清單」對上游現場抓（`catalog_fetch`）。
 * 這裡的每一個 `base_url` 都在後端 `presets.rs` 註明是實測過的（見該檔開頭）。
 */
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, type Preset } from "../../lib/api";
import { Icon } from "../icons";

export function PresetPicker(props: {
  onPick: (p: Preset) => void;
  onCustom: () => void;
}) {
  const [q, setQ] = useState("");
  const presets = useQuery({ queryKey: ["presets"], queryFn: api.presetsList });

  const list = useMemo(() => {
    const all = presets.data ?? [];
    const s = q.trim().toLowerCase();
    if (!s) return all;
    return all.filter(
      (p) =>
        p.name.toLowerCase().includes(s) ||
        p.base_url.toLowerCase().includes(s) ||
        p.id.toLowerCase().includes(s),
    );
  }, [presets.data, q]);

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Icon
            name="search"
            size={13}
            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-fg/25"
          />
          <input
            className="field w-full py-1.5 pr-2 pl-8 text-xs"
            placeholder="搜尋預設（名稱、網址或 id）"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <button
          className="btn-ghost flex shrink-0 items-center gap-1 px-3 py-1.5 text-xs"
          onClick={props.onCustom}
        >
          <Icon name="pencil" size={11} />
          自訂
        </button>
      </div>

      {presets.isPending ? (
        <p className="px-1 py-3 text-center text-xs text-fg/30">載入中…</p>
      ) : presets.isError ? (
        <p className="px-1 py-3 text-center text-xs text-fg/45">
          讀取預設失敗：{String(presets.error)}
        </p>
      ) : list.length === 0 ? (
        <p className="px-1 py-3 text-center text-xs text-fg/30">
          沒有符合的預設，改按「自訂」自己填。
        </p>
      ) : (
        <div className="max-h-[34vh] space-y-1 overflow-y-auto pr-0.5">
          {list.map((p) => (
            <button
              key={p.id}
              className="flex w-full flex-col gap-0.5 rounded-lg border border-fg/[0.06] px-2.5 py-2 text-left transition-colors hover:border-fg/15 hover:bg-fg/[0.04]"
              onClick={() => props.onPick(p)}
            >
              <span className="flex w-full items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-fg/85">
                  {p.name}
                </span>
                <span className="shrink-0 rounded bg-fg/[0.06] px-1.5 py-px font-mono text-[10px] text-fg/40">
                  {p.api_format}
                </span>
              </span>
              <span className="w-full truncate font-mono text-[11px] text-fg/30">
                {p.base_url}
              </span>
              {p.note && (
                <span className="w-full text-[11px] leading-snug text-amber-300/80">
                  {p.note}
                </span>
              )}
            </button>
          ))}
        </div>
      )}

      <p className="pt-1 text-[11px] leading-relaxed text-fg/25">
        選了預設只會填好連線資訊（名稱、上游地址、協議、鑑權），金鑰請自己貼；
        模型清單按表單上的「取得模型清單」現場抓。全部 {presets.data?.length ?? 0} 個預設的
        上游位址都已實測回應（見文件 §0.9.20）。
      </p>
    </div>
  );
}
