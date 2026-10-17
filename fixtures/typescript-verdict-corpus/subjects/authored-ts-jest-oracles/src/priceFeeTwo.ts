export const FLAT = 499;

export function priceFeeTwo(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
