export function priceNamedRound(amount: number): number {
  if (amount > 100) {
    return amount - 10;
  }
  return amount;
}

export default function round(amount: number): number {
  return Math.round(amount);
}
