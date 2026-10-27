export function pick(s: number, admin: boolean): number {
  if (!admin) {
    if (s >= 404) { return 1; }
    return 2;
  }
  return 3;
}
