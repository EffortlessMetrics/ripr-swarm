export const FLAT = 499;

export function priceFeeSign(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
