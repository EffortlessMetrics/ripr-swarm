export function announceRetry(id: number, send: (message: string) => void): void {
  send("order " + id);
  send("order " + id);
}
