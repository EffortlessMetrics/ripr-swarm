import { test } from "node:test";
import assert from "node:assert/strict";
import { Stats } from "../src/delete.ts";

test("one hit is counted", () => {
  const stats = new Stats();
  stats.recordHit();
  assert.equal(stats.hits, 1);
});

test("miss table", () => {
  const rows: Array<[number, number]> = [[0, 0], [1, 1], [3, 3]];
  for (const [calls, want] of rows) {
    const stats = new Stats();
    for (let i = 0; i < calls; i++) {
      stats.recordMiss();
    }
    assert.equal(stats.misses, want);
  }
});

test("reads never go negative", () => {
  const stats = new Stats();
  for (let i = 0; i < 3; i++) {
    stats.recordRead();
  }
  assert.ok(stats.reads >= 0);
});

function checkWrites(stats: Stats, want: number): void {
  assert.equal(stats.writes, want);
}

test("writes are counted", () => {
  const stats = new Stats();
  stats.recordWrite();
  stats.recordWrite();
  checkWrites(stats, 2);
});

test("eviction smoke", () => {
  const stats = new Stats();
  stats.recordEviction();
});
