export function rate(tier: string): number {
  return tier === "gold" ? 0.2 : 0.1;
}
