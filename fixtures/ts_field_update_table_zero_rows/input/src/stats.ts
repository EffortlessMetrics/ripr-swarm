export class Stats {
  misses = 0;
  recordMiss(): void {
    this.misses += 1;
  }
}
