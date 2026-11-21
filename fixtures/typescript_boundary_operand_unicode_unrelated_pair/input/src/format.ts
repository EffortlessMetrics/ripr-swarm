export function formatShort(時: number, unused: number, limit: number): string {
  if (時 >= limit) {
    return "1h";
  }
  return "short";
}
