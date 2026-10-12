export function parseCoupon(code: string): number {
  if (!code.startsWith("SAVE")) {
    throw new Error("unknown coupon prefix");
  }
  return Number(code.slice(4));
}
