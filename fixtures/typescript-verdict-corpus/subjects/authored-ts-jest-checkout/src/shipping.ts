export function shippingFee(weight: number): number {
  if (weight > 20) {
    return 15;
  }
  return 5;
}
