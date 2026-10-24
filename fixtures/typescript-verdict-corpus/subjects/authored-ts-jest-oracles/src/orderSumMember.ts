export async function orderSumMember(items: number[]): Promise<number> {
  return items.reduce((sum, item) => sum + item, 0);
}
