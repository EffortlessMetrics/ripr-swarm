export async function chargeCard(amount: number): Promise<number> {
  if (amount <= 0) {
    throw new Error("charge must be positive");
  }
  return amount;
}
