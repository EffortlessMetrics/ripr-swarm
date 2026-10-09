export async function loadLimit(tier: string): Promise<number> {
  await Promise.resolve();
  if (tier === "gold") {
    return 5000;
  }
  return 1000;
}
