//! 由 `components/Diagnostics.tsx` 拆分而來（原檔 563 行）。程式碼語意未改，只搬位置。

export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function fmtTime(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function fmtBytes(n: number | null): string {
  if (n === null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}
