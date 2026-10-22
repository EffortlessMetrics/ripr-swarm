import { test } from "node:test";
import assert from "node:assert/strict";
import { skippedTest } from "../src/core.ts";

test.skip("skipped total", () => {
  assert.equal(skippedTest(3, 7), 21);
});
