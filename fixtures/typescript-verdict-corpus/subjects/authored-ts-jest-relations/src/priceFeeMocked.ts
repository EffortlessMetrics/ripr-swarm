import { fee31 } from "./fee31";

export function priceFeeMocked(amount: number): number {
  if (amount > 100) {
    return fee31(amount - 10);
  }
  return amount;
}
