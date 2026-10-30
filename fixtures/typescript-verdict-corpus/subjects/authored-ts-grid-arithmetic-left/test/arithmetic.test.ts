import { test } from "node:test";
import assert from "node:assert/strict";
import { scaled } from "../src/arithmetic.ts";

test("scaling four by one keeps four", () => {
  assert.equal(scaled(4, 1), 4);
});
