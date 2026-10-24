export function applyDiscountNamed(amount: number): number {
  return amount - Math.floor(amount / 10);
}
