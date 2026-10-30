import { test } from "node:test";
import assert from "node:assert/strict";
import { skipOption } from "../src/core.ts";

test("skip option total", { skip: false }, () => {
  assert.equal(skipOption(3, 7), 21);
});
