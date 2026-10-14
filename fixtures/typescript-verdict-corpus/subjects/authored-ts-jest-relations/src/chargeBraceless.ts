export interface BracelessNotifier {
  send(owner: string, message: string): void;
}

export function chargeBraceless(owner: string, cents: number[], notifier: BracelessNotifier): number {
  const total = cents.reduce((sum, c) => sum + c, 0);
  if (total > 0)
    notifier.send(owner, "charged " + total);
  return total;
}
