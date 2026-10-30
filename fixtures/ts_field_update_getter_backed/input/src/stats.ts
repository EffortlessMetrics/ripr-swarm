export class Stats {
  private stored = 0;
  get hits(): number {
    return this.stored;
  }
  set hits(value: number) {
    this.stored = Math.min(value, 5);
  }
  recordHit(): void {
    this.hits += 1;
  }
}
