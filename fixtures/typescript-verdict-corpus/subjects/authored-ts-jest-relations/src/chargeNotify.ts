export interface ChargeNotifier {
  send(owner: string, message: string): void;
  flush(): void;
}

export function chargeNotify(owner: string, cents: number[], notifier: ChargeNotifier): number {
  const total = cents.reduce((sum, c) => sum + c, 0);
  notifier.send(owner, "charged " + total);
  return total;
}
