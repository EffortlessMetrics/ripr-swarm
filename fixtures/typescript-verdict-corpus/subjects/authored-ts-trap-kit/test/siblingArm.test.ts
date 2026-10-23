import { test } from "node:test";
import assert from "node:assert/strict";
import { siblingArm } from "../src/core.ts";

test("bulk rate", () => {
  assert.equal(siblingArm("bulk", 4), 12);
});
