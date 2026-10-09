export function refundAmount(amount: number): number {
  if (amount < 0) {
    throw new Error("negative refund");
  }
  return amount;
}
