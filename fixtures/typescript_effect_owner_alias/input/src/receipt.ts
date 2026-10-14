export function emitReceipt(amount: number): number {
  console.log("audit", amount * 9);
  return amount - 10;
}
