export const calls: string[] = [];

export function audited(_target: object, key: string, descriptor: PropertyDescriptor): void {
  const original = descriptor.value as (...args: number[]) => number;
  descriptor.value = function (this: unknown, ...args: number[]): number {
    calls.push(key);
    return original.apply(this, args);
  };
}

export class Ledger {
  @audited
  add(a: number, b: number): number {
    return a + b;
  }
}
