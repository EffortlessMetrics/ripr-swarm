export function entry(): number {
  class Counter {
    #limit(value: number): number {
      return value >= 50 ? 55 : 0;
    }
    run(): number { return this.#limit(60); }
  }
  return new Counter().run();
}
