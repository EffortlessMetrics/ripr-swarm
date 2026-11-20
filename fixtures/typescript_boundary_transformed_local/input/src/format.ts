export function formatShort(ms: number): string {
  const msAbs = Math.abs(ms) / 2;
  if (msAbs >= 3600000) {
    return "1h";
  }
  return "short";
}
