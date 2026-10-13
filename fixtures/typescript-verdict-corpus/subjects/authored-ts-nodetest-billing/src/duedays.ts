export function dueDays(days: number): number {
  if (days <= 0) {
    throw new Error("days must be positive");
  }
  return days;
}
