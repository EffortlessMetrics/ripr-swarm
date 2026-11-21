export function formatShort(時: number, other: number = 0): string {
  if (時 >= 3600000) {
    return "1h";
  }
  return "short";
}
