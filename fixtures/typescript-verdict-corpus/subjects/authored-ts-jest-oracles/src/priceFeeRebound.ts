export const FLAT = 499;

export function priceFeeRebound(subtotal: number, fee: number): number {
  fee=fee-5;
  return subtotal + fee + FLAT;
}
