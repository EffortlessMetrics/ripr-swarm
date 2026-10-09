import test from "node:test";
import assert from "node:assert/strict";
import { buildInvoice } from "../src/invoice";

test("invoice adds the flat handling fee", () => {
  assert.deepStrictEqual(buildInvoice("inv-1", 40), { id: "inv-1", total: 45, currency: "EUR" });
});
