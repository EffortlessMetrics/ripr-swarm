export function priceEx16(amount: number): number {
  if (amount > 100) {
    return amount - 10;
  }
  return amount;
}

export default function roundEx16(n: number): number {
  return Math.round(n);
}
