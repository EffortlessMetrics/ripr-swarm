export function priceEx15(amount: number): number {
  if (amount > 120) {
    throw new Error("bad");
  }
  if (amount > 100) {
    return amount - 10;
  }
  return amount;
}
