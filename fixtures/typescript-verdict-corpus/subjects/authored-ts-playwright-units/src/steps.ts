export function shipping(weightKg: number): number {
  return weightKg > 5 ? 900 : 400;
}
