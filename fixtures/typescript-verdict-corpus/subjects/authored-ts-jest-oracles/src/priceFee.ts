export const FLAT = 499;

export function priceFee(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
