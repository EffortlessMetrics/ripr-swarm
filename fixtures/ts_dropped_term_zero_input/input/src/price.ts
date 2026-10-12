export const FLAT = 499;

export function shipping(subtotal: number): number {
  return subtotal >= 5000 ? 0 : FLAT;
}

export function price(subtotal: number, fee: number): number {
  return subtotal + shipping(subtotal) + FLAT;
}
