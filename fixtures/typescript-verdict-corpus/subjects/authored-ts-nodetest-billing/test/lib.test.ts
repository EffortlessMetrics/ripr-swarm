import test from "node:test";
import assert from "node:assert/strict";
import { price } from "../src/lib";

await test("price", () => {
  assert.equal(price(150), 140);
});
