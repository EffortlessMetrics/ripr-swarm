export function checkQuantity(qty: number): number {
  if (qty <= 0) {
    throw new Error("quantity must be positive");
  }
  return qty;
}
