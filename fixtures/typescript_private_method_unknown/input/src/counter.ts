export class Counter {
  #limit(value: number): number {
    return value >= 50 ? 55 : 0;
  }
  entry(value: number): number { return this.#limit(value); }
}
