export function sumAll(values: number[]): number {
  let total = 0;
  for (const v of values) {
    total += v;
  }
  return total;
}
