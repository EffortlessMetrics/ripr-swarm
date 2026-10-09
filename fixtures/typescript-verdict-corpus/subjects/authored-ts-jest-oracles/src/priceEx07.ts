export function priceEx07(amount: number): number {
  if (amount > 100) {
    return amount - 10;
  }
  return amount;
}
