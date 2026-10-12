export function orderTotal(qty: number, unit: number, fee: number): number {
  const subtotal = qty * unit;
  return subtotal + fee;
}
