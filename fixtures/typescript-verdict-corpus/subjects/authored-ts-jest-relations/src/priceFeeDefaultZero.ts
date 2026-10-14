export const FLAT = 499;

export default function priceFeeDefaultZero(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
