export async function fetchUnitPriceLocal(sku: string): Promise<number> {
  const table: Record<string, number> = { apple: 120, pear: 95 };
  return table[sku] ?? 0;
}
