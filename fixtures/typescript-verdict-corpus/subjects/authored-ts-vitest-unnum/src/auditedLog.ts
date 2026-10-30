export function chargeAudited(amount: number, percent: number): number {
  console.log("audit", amount * 9);
  return amount - Math.floor((amount * percent) / 100);
}
