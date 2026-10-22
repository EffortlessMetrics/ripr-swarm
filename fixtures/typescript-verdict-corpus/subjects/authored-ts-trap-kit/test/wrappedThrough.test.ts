import { test } from "node:test";
import assert from "node:assert/strict";
import { wrappedThrough } from "../src/core.ts";

function logged(fn: () => void): void {
  try {
    fn();
  } catch (error) {
    throw error;
  }
}

test("wrapped through", () => {
  logged(() => assert.equal(wrappedThrough(3, 7), 21));
});
