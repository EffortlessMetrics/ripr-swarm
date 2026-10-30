export function feeCents(amount: number): number {
  return amount > 0 ? Math.floor(amount * 100) : 0;
}
