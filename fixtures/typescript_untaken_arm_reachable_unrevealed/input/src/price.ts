export function price(qty: number, coupon?: string): number {
  let total = qty * 10;
  if (coupon === 'HALF') {
    total = total * qty;
  }
  return total;
}
