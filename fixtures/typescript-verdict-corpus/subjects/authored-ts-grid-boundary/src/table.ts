export function qualifiesForFreeShipping(cents: number): boolean {
  return cents >= 5000;
}

export function qualifiesForFreeReturns(cents: number): boolean {
  return cents >= 8000;
}

export function qualifiesForGiftWrap(cents: number): boolean {
  return cents >= 3000;
}
