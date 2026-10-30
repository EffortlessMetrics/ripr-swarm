import { test } from "node:test";
import assert from "node:assert/strict";
import { Stats } from "../src/stats.ts";

test("one hit", () => {
  const stats = new Stats();
  stats.recordHit();
  assert.equal(stats.hits, 1);
});
