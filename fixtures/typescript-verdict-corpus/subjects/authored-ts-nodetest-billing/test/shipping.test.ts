import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { shippingCost } from "../src/shipping";

describe("shippingCost", () => {
  it("charges the heavy rate far above the limit", () => {
    assert.equal(shippingCost(30), 15);
  });
  it("charges the light rate far below the limit", () => {
    assert.equal(shippingCost(5), 5);
  });
});
