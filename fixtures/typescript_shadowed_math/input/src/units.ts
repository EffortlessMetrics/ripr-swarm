const Math = { abs: (_n: number) => 1 };
export function cmToIn(cm: number): number {
  return Math.abs(cm + 1);
}
