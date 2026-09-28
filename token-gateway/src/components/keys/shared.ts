//! 由 `Keys.tsx` 拆分而來。程式碼語意未改，只搬位置。

export function copyText(t: string) {
  void navigator.clipboard?.writeText(t).catch(() => {});
}

export function fmtTokens(n: number): string {
  return n < 0 ? "不限" : n.toLocaleString();
}
