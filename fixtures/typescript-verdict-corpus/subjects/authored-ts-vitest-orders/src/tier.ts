export function isGoldTier(spend: number): boolean {
  if (spend >= 500) {
    return true;
  }
  return false;
}
