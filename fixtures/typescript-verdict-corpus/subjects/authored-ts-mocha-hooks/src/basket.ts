export class Basket {
  private items: number[] = [];

  add(price: number): void {
    this.items.push(price);
  }

  total(): number {
    return this.items.reduce((sum, p) => sum + p, 0);
  }

  count(): number {
    return this.items.length;
  }
}
