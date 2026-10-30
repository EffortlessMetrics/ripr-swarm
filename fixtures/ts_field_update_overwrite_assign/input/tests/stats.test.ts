import { test } from "node:test";
import assert from "node:assert/strict";
import { Stats } from "../src/stats.ts";

test("overwritten count", () => {
  const stats = new Stats();
  stats.recordHit();
  Object.assign(stats, { hits: 9 });
  assert.equal(stats.hits, 9);
});
