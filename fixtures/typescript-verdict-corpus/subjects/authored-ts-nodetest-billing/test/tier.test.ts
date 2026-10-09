import test from "node:test";
import assert from "node:assert/strict";
import { memberTier } from "../src/tier";

test("gold starts exactly at 500 points", () => {
  assert.strictEqual(memberTier(499), "basic");
  assert.strictEqual(memberTier(500), "gold");
});
