export function memoize(_target: object, _key: string, descriptor: PropertyDescriptor): void {
  const original = descriptor.value as (n: number) => number;
  const cache = new Map<number, number>();
  descriptor.value = function (this: unknown, n: number): number {
    if (!cache.has(n)) {
      cache.set(n, original.call(this, n));
    }
    return cache.get(n) as number;
  };
}

export class Calc {
  @memoize
  square(n: number): number {
    return n * n;
  }
}
