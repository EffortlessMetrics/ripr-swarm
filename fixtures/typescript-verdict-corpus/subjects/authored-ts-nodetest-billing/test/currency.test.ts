import test from "node:test";
import assert from "node:assert/strict";
import { currencyCode } from "../src/currency";

test("unsupported currency names the problem", () => {
  assert.throws(() => currencyCode("XYZ"), /^Error: unsupported currency$/);
});
