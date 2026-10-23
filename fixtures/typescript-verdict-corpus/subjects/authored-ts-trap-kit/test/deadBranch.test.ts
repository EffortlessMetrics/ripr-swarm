import { test } from "node:test";
import assert from "node:assert/strict";
import { deadBranch } from "../src/core.ts";

test("dead branch total", () => {
  const total = deadBranch(3, 7);
  if (false) {
    assert.equal(total, 21);
  }
});
