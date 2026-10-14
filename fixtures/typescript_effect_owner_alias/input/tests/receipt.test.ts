import assert from "node:assert/strict";
import { test } from "node:test";
import { emitReceipt as discount } from "../src/receipt.ts";
test("unchanged return is not the audit event", () => {
  assert.strictEqual(discount(100), 90);
});
