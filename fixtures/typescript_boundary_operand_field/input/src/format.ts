export function formatShort(ms: number, other: number = 0): string {
  const o = { ms: ms / 2 };
  if (o.ms >= 3600000) {
    return "1h";
  }
  return "short";
}
