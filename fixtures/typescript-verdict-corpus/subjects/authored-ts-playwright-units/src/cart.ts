export class Cart {
  private cents: number[] = [];

  add(amount: number): void {
    this.cents.push(amount);
  }

  total(): number {
    return this.cents.reduce((a, b) => a + b, 0);
  }
}
