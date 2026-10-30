export function salesTax(subtotal: number): number {
  return Math.floor((subtotal * 8) / 100);
}

export function lateFee(days: number): number {
  return days * 5;
}

export function commission(sale: number): number {
  return Math.floor((sale * 3) / 100);
}

export function handling(weight: number): number {
  return weight * 12;
}

export function roundingUnit(value: number): number {
  return Math.floor(value / 10) * 10;
}
