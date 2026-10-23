import assert from "node:assert/strict";
import { test } from "node:test";
import { weight, Other as Level } from "../src/level.ts";
test("high", () => {
  assert.strictEqual(weight(Level.High), 10);
});
