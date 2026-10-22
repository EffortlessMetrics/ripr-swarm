export function shippingCents(weightGrams: number): number {
  if (weightGrams <= 500) {
    return 399;
  }
  return 899;
}
