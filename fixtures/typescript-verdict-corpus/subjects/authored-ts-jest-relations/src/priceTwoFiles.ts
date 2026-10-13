import { fee33 } from "./fee33";

export function priceTwoFiles(amount: number): number {
  if (amount > 100) {
    return fee33(amount - 10);
  }
  return amount;
}
