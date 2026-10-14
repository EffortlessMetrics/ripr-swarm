export interface IfNotifier {
  send(owner: string, message: string): void;
}

export function chargeAfterIf(owner: string, cents: number[], notifier: IfNotifier): number {
  const total = cents.reduce((sum, c) => sum + c, 0);
  if (total < 0) {
    throw new RangeError("negative total");
  }
  notifier.send(owner, "charged " + total);
  return total;
}
