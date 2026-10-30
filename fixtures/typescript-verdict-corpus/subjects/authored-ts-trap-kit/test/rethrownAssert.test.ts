import { test } from "node:test";
import assert from "node:assert/strict";
import { rethrownAssert } from "../src/core.ts";

test("rethrown assert", () => {
  try {
    assert.equal(rethrownAssert(3, 7), 21);
  } catch (error) {
    throw error;
  }
});
