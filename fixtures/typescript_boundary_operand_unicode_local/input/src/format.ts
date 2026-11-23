export function formatShort(ms: number, other: number = 0): string {
  const 時 = ms / 2;
  if (時 >= 3600000) {
    return "1h";
  }
  return "short";
}
