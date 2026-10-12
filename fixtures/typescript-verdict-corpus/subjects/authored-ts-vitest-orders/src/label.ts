export function shippingLabel(name: string, qty: number): string {
  const units = qty === 1 ? "unit" : "units";
  return `${name} (${qty} ${units})`;
}
