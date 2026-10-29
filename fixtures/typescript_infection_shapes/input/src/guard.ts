export function safeDiv(a: number, b: number): number {
  if (b === 0) {
    return a;
  }
  return a / b;
}
export function floorAt(x: number, lo: number): number {
  if (x < lo) {
    return lo;
  }
  return x;
}
