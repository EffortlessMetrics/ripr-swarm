import { fee32 } from "./fee32";

export function priceUnrelatedMocked(amount: number): number {
  if (amount > 100) {
    return fee32(amount - 10);
  }
  return amount;
}
