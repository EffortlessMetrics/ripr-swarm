export interface TimesNotifier {
  send(owner: string, message: string): void;
}

export function chargeTimes(owner: string, cents: number[], notifier: TimesNotifier): number {
  const total = cents.reduce((sum, c) => sum + c, 0);
  notifier.send(owner, "charged " + total);
  return total;
}
