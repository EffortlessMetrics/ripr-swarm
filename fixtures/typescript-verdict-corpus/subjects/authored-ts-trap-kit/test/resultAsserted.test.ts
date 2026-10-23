import { test } from "node:test";
import assert from "node:assert/strict";
import { resultAsserted } from "../src/core.ts";

test("result asserted", () => {
  const a = 3;
  const b = 7;
  assert.equal(resultAsserted(a, b), 21);
});
