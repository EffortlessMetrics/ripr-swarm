export function announceShipment(id: number, send: (message: string) => void): void {
  send(`order ${id} shipped`);
}
