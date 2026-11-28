export function formatShort(ms: number, other: number = 0): string {
  const msAbs = Math.abs(ms);
  if (msAbs >= 3600000) {
    return "1h";
  }
  return "short";
}
