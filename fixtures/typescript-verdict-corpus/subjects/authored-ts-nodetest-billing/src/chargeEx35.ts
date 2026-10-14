export async function chargeEx35(n: number): Promise<number> {
  if (n < 0) {
    throw new Error("charge must be nonnegative");
  }
  return n;
}
