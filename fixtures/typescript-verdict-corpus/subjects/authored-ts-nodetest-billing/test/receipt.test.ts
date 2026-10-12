import test from "node:test";
import assert from "node:assert";
import { receiptLine } from "../src/receipt";

test("receipt line labels the count", () => {
  assert.deepEqual(receiptLine(3), { count: 3, label: "items: 3" });
});
