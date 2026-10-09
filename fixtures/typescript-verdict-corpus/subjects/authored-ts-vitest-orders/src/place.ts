import { reserveStock } from "./stock";

export function placeOrder(onHand: number, qty: number): string {
  const left = reserveStock(onHand, qty);
  return left >= 0 ? "placed" : "backordered";
}
