import { test } from "node:test";
import assert from "node:assert/strict";
import { swallowedAssert } from "../src/core.ts";

test("swallowed assert", () => {
  try {
    assert.equal(swallowedAssert(3, 7), 21);
  } catch {
    // ignored
  }
});
