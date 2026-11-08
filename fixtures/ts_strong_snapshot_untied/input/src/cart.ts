export class Cart {
  total = 0;
  add(amount: number): number {
    this.total += amount * 2;
    return this.total;
  }
}
