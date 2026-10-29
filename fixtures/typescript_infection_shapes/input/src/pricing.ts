export function bulk(qty: number): number {
  if (qty >= 10) {
    return 5;
  }
  return 0;
}
export function tier(score: number): string {
  return score > 50 ? "high" : "low";
}
export function label(n: number): string {
  return `n=${n}`;
}
export function total(xs: number[]): number {
  let t = 0;
  for (const x of xs) {
    t += x;
  }
  return t;
}
export function div(a: number, b: number): number {
  if (b === 0) {
    return a;
  }
  return a / b;
}
export function cap(opts?: { max?: number }): number {
  return opts?.max ?? 10;
}
export function disc(price: number): number {
  return Math.floor(price * 0.9);
}
export function say(msg: string, loud = false): string {
  return loud ? msg.toUpperCase() : msg;
}
export function sum(items: number[]): number {
  return items.reduce((a, b) => a + b, 0);
}
export function same(price: number): number {
  return price;
}
export const LIMIT = 5;
