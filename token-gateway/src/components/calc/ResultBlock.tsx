import type { PriceQuote } from "../../lib/api";
import { Icon } from "../icons";
import { SideCard } from "./SideCard";

export type Saving = { cheap: "A" | "B"; diff: number; pct: number };

export function ResultBlock(props: {
  a: PriceQuote;
  b: PriceQuote;
  aLabel: string;
  bLabel: string;
  saving: Saving | null;
}) {
  const saving = props.saving;
  return (
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
          quote={props.a}
          pending={false}
          winner={!!saving && saving.cheap === "A"}
          modelLabel={props.aLabel}
        />
        <SideCard
          title="B"
          quote={props.b}
          pending={false}
          winner={!!saving && saving.cheap === "B"}
          modelLabel={props.bLabel}
        />
      </div>
    </div>
  );
}
