export function clampPercent(n: number): number {
  return Math.min(100, Math.max(0, n));
}
