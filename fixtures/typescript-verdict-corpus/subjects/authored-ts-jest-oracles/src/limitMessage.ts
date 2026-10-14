export function limitMessage(amount: number, max: number): number {
  if (amount > max) {
    throw new RangeError("limit " + max);
  }
  return amount;
}
