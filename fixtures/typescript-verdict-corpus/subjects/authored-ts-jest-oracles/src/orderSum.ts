export async function orderSum(items: number[]): Promise<number> {
  return items.reduce((sum, item) => sum + item, 0);
}
