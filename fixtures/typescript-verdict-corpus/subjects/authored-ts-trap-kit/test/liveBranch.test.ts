import { test } from "node:test";
import assert from "node:assert/strict";
import { liveBranch } from "../src/core.ts";

test("live branch total", () => {
  const total = liveBranch(3, 7);
  if (true) {
    assert.equal(total, 21);
  }
});
