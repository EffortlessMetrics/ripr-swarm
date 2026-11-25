export function formatShort(ms: number, other: number = 0): string {
  if (ms / 2 >= 3600000) {
    return "1h";
  }
  return "short";
}
