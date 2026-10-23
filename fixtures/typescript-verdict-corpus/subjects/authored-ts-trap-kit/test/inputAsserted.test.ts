import { test } from "node:test";
import assert from "node:assert/strict";
import { inputAsserted } from "../src/core.ts";

test("input asserted", () => {
  const a = 3;
  const b = 7;
  const result = inputAsserted(a, b);
  assert.equal(a, 3);
  assert.equal(b, 7);
  assert.notEqual(result, undefined);
});
