export class Stats {
  hits = 0;
  misses = 0;
  reads = 0;
  writes = 0;
  evictions = 0;
  lastEvent: string | null = null;

  recordHit(): void {
    this.hits = this.hits + 1;
    this.lastEvent = "hits";
  }

  recordMiss(): void {
    this.misses = this.misses + 1;
    this.lastEvent = "misses";
  }

  recordRead(): void {
    this.reads = this.reads + 1;
    this.lastEvent = "reads";
  }

  recordWrite(): void {
    this.writes = this.writes + 1;
    this.lastEvent = "writes";
  }

  recordEviction(): void {
    this.evictions = this.evictions + 1;
    this.lastEvent = "evictions";
  }
}
