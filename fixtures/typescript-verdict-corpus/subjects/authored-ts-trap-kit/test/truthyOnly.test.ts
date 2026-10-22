import { test } from "node:test";
import assert from "node:assert/strict";
import { truthyOnly } from "../src/core.ts";

test("truthy only", () => {
  assert.ok(truthyOnly(3, 7));
});
