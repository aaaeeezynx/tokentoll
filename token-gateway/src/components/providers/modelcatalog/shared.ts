//! 由 `ModelCatalog.tsx` 拆分而來（原檔 613 行）。程式碼語意未改，只搬位置。

export function fmtCatalogTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts);
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
