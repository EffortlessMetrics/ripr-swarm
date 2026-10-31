export function applyDiscount(total: number, ready: boolean): number {
  if ((total >= 100) || ready) {
    return total * 0.9;
  }
  return total;
}
