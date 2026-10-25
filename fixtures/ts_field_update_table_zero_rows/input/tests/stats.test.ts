import { test } from "node:test";
import assert from "node:assert/strict";
import { Stats } from "../src/stats.ts";

test("miss table", () => {
  const rows: Array<[number, number]> = [[0, 0], [0, 0]];
  for (const [calls, want] of rows) {
    const stats = new Stats();
    for (let i = 0; i < calls; i++) {
      stats.recordMiss();
    }
    assert.equal(stats.misses, want);
  }
});
