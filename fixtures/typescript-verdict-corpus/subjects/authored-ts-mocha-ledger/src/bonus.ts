export function bonusPoints(spend: number): number {
  if (spend > 500) {
    return 25;
  }
  return 0;
}
