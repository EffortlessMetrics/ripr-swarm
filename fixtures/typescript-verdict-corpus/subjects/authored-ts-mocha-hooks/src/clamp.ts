export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

export function mean(xs: number[]): number {
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}

export function describeRange(lo: number, hi: number): string {
  if (lo > hi) {
    throw new RangeError("lo above hi");
  }
  return `${lo}..${hi}`;
}
