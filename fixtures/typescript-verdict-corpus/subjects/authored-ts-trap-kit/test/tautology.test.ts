import { test } from "node:test";
import assert from "node:assert/strict";
import { tautology } from "../src/core.ts";

test("tautology", () => {
  assert.equal(tautology(3, 7), tautology(3, 7));
});
