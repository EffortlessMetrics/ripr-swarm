import { test } from "node:test";
import assert from "node:assert/strict";
import { todoTest } from "../src/core.ts";

test("todo total", { todo: true }, () => {
  assert.equal(todoTest(3, 7), 21);
});
