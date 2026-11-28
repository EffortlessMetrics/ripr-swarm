export function discount(total: number): number {
  if (total > 100) {
    return total - 10;
  }
  return total;
}
