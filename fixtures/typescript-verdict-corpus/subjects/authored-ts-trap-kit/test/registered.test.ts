import { test } from "node:test";
import assert from "node:assert/strict";
import { registered } from "../src/core.ts";

function checkRegisteredTotal(): void {
  assert.equal(registered(3, 7), 21);
}

test("registered total", () => {
  checkRegisteredTotal();
});
