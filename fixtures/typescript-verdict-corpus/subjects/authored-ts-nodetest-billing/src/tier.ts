export function memberTier(points: number): string {
  if (points >= 500) {
    return "gold";
  }
  return "basic";
}
