export function price(a: number, b: number): number {
  if (b === 0) {
    return a;
  }
  return a / b;
}
