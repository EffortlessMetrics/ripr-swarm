export function rank(score: number): string {
  return score >= 90 ? "gold" : "silver";
}

export function medal(score: number): string {
  return score >= 50 ? "bronze" : "none";
}
