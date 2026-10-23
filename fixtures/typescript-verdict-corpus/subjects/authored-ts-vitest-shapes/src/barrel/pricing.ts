export function withTax(net: number): number {
  return net + Math.round(net * 0.2);
}
