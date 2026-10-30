export function safeDiv(a: number, b: number): number {
  if (b === 0) {
    return b;
  }
  return a / b;
}
export function floorAt(x: number, lo: number): number {
  if (x < lo) {
    return x;
  }
  return x;
}
