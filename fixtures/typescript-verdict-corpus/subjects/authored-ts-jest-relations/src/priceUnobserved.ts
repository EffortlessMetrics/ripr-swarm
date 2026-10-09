export function priceUnobserved(amount: number): number {
  if (amount > 100) {
    return amount - 10;
  }
  return amount;
}
