export const TAX = { US: 0.07, DE: 0.19 } as const;

export function tax(country: keyof typeof TAX, amount: number): number {
  return amount * TAX[country];
}
