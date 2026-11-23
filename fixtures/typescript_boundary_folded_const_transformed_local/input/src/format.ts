const s = 1000;
const m = s * 60;
const h = m * 60;

export function formatShort(ms: number): string {
  const msAbs = Math.abs(ms) / 2;
  if (msAbs >= h) {
    return "1h";
  }
  return "short";
}
