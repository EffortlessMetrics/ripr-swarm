export function chargeObserved(amount: number, percent: number): number {
  console.log("observed", amount * 9);
  return amount - Math.floor((amount * percent) / 100);
}
