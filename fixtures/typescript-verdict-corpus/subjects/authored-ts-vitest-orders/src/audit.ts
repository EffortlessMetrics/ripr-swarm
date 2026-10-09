export function recordShipment(log: (message: string) => void, orderId: number): void {
  log("shipped order " + orderId);
}
