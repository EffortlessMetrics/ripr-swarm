export class Stats {
  hits = 0;
  recordHit(): void {
    this.hits += 1;
  }
}
