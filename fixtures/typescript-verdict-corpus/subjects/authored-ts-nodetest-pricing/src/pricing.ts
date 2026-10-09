export const FREE_SHIPPING_CENTS = 5000;
export const FLAT_SHIPPING_CENTS = 499;

export function shippingFee(subtotalCents: number): number {
  if (subtotalCents >= FREE_SHIPPING_CENTS) {
    return 0;
  }
  return FLAT_SHIPPING_CENTS;
}

export function tierForPoints(points: number): string {
  if (points >= 5000) {
    return "gold";
  }
  if (points >= 1000) {
    return "silver";
  }
  return "bronze";
}

export function discountCents(tier: string, subtotalCents: number): number {
  if (tier === "gold") {
    return Math.floor((subtotalCents * 10) / 100);
  }
  if (tier === "silver") {
    return Math.floor((subtotalCents * 5) / 100);
  }
  return 0;
}

export function taxCents(subtotalCents: number): number {
  return Math.floor((subtotalCents * 8) / 100);
}

export function totalWithTax(subtotalCents: number): number {
  return subtotalCents + taxCents(subtotalCents);
}

export function parseQuantity(text: string): number {
  const value = Number.parseInt(text, 10);
  if (value <= 0) {
    throw new RangeError("quantity must be positive");
  }
  return value;
}

export function chargeWithAudit(amountCents: number, rate: number): number {
  console.log("audit", amountCents * rate);
  return amountCents - Math.floor((amountCents * rate) / 100);
}

export interface Quote {
  tier: string;
  totalCents: number;
}

export function quote(subtotalCents: number, points: number): Quote {
  const tier = tierForPoints(points);
  const total = subtotalCents - discountCents(tier, subtotalCents);
  return { tier, totalCents: total + shippingFee(subtotalCents) };
}

export function receiptFooter(storeName: string): string {
  return `Thank you for shopping at ${storeName}!`;
}
