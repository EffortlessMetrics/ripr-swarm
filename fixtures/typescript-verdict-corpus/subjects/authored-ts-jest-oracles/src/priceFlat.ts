export const FLAT = 499;

export function priceFlat(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
