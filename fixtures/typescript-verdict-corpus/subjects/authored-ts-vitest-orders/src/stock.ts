export function reserveStock(onHand: number, qty: number): number {
  return onHand - qty;
}
