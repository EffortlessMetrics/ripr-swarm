export async function orderSumMultiline(items: number[]): Promise<number> {
  return items.reduce((sum, item) => sum + item, 0);
}
