export async function fetchUnitPriceNull(sku: string): Promise<number> {
  const table: Record<string, number | null> = { apple: 120, pear: 95 };
  return table[sku] ?? 0;
}
