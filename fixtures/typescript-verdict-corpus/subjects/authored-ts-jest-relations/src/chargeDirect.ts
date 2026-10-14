export interface DirectNotifier {
  send(owner: string, message: string): void;
}

export function chargeDirect(owner: string, cents: number[], notifier: DirectNotifier): number {
  const total = cents.reduce((sum, c) => sum + c, 0);
  notifier.send(owner, "charged " + total);
  return total;
}
