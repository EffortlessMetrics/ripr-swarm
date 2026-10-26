export function ceil(n: number, step: number): number {
  return Math.ceil(n / step) * step;
}

export function bucket(n: number): string {
  return n < 10 ? "small" : "large";
}
