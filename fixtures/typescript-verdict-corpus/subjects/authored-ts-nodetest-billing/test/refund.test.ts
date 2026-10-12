import test from "node:test";
import assert from "node:assert/strict";
import { refundAmount } from "../src/refund";

test("a negative refund throws", () => {
  assert.throws(() => refundAmount(-3));
});
