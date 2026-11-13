/** Orders at or above the threshold get the discount. */
export const DISCOUNT_THRESHOLD = 10000;

export function discountedTotal(subtotal: number): number {
  if (subtotal >= DISCOUNT_THRESHOLD) {
    return subtotal - 500;
  }
  return subtotal;
}
