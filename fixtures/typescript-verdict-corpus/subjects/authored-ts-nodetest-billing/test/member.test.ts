import test from "node:test";
import assert from "node:assert/strict";
import { memberPrice } from "../src/member";

test("member pricing", async (t) => {
  await t.test("takes five off the list price", () => {
    assert.strictEqual(memberPrice(30), 25);
  });
});
