import test from "node:test";
import assert from "node:assert/strict";
import { isBulkOrder } from "../src/bulk";

test("a large order is bulk", () => {
  assert.ok(isBulkOrder(50));
});
