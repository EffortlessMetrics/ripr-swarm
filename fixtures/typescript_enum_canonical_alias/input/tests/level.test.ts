import assert from "node:assert/strict";
import { test } from "node:test";
import { weight, Level as Rank } from "../src/level.ts";
test("high", () => {
  assert.strictEqual(weight(Rank.High), 10);
});
