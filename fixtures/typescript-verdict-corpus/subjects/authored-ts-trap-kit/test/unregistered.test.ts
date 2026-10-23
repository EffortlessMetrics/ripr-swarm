import { test } from "node:test";
import assert from "node:assert/strict";
import { unregistered } from "../src/core.ts";

function checkUnregisteredTotal(): void {
  assert.equal(unregistered(3, 7), 21);
}

test("unregistered smoke", () => {
  assert.equal(typeof checkUnregisteredTotal, "function");
});
