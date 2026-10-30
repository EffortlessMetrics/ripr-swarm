export function discountedTotal(amount: number): number {
  if (amount >= 10000) {
    return amount - Math.floor(amount / 10);
  }
  return amount;
}

export function loyaltyPrice(amount: number, years: number): number {
  if (years >= 5) {
    return amount - Math.floor(amount / 20);
  }
  return amount;
}
