export const FLAT = 499;

export function priceFeeZero(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
