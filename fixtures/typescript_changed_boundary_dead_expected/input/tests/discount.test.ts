import assert from "node:assert/strict";
import { test } from "node:test";
import { applyDiscount } from "../src/discount.ts";
test("dead expected boundary value", () => {
  assert.strictEqual(applyDiscount(100, false), 999);
});
