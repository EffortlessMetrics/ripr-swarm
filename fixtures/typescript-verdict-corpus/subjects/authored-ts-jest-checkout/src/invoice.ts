import { salesTax } from "./tax";

export function invoiceTotal(amount: number): number {
  return amount + salesTax(amount);
}
