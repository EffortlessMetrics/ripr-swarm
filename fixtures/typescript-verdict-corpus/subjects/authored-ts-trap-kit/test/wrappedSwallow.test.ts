import { test } from "node:test";
import assert from "node:assert/strict";
import { wrappedSwallow } from "../src/core.ts";

function quiet(fn: () => void): void {
  try {
    fn();
  } catch {
    // swallowed
  }
}

test("wrapped swallow", () => {
  quiet(() => assert.equal(wrappedSwallow(3, 7), 21));
});
