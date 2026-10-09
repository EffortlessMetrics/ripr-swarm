const BASE: Record<string, number> = { apple: 10, pear: 12 };

export async function priceOf(sku: string): Promise<number> {
  const base = BASE[sku] ?? 0;
  return base * 2;
}
