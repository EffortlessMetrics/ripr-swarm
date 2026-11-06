import { test } from "node:test";
import assert from "node:assert/strict";
import { price } from "../src/price.ts";

test("positive price", () => {
  assert.ok(price());
});
