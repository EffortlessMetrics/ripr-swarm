export function discountedTotal(amount: number, threshold: number): number {
  return amount >= threshold ? amount - 10 : amount;
}
