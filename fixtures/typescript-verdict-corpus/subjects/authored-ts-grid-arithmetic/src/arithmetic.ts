export function lineTotal(qty: number, price: number): number {
  return qty * price;
}

export function area(width: number, height: number): number {
  return width * height;
}

export function scaled(qty: number, unit: number): number {
  return qty * unit;
}

export function tip(bill: number, rate: number): number {
  return bill * rate;
}

export function seconds(minutes: number): number {
  return minutes * 60;
}
