export class Counter {
  private value = 0;

  private bump(n: number): number {
    return n + 1;
  }

  next(): number {
    this.value = this.bump(this.value);
    return this.value;
  }
}
