export const FLAT = 499;

export default function priceFeeDefault(subtotal: number, fee: number): number {
  return subtotal + fee + FLAT;
}
