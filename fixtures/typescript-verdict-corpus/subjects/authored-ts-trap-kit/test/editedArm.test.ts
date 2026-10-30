import { test } from "node:test";
import assert from "node:assert/strict";
import { editedArm } from "../src/core.ts";

test("single rate", () => {
  assert.equal(editedArm("single", 5), 10);
});
