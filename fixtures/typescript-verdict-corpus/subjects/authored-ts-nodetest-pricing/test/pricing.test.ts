import { test } from "node:test";
import assert from "node:assert/strict";
import {
  chargeWithAudit,
  discountCents,
  parseQuantity,
  quote,
  shippingFee,
  taxCents,
  tierForPoints,
  totalWithTax,
} from "../src/pricing.ts";

test("free shipping starts at the threshold", () => {
  assert.equal(shippingFee(5000), 0);
  assert.equal(shippingFee(4999), 499);
});

test("tiers far from their thresholds", () => {
  assert.equal(tierForPoints(6000), "gold");
  assert.equal(tierForPoints(0), "bronze");
});

test("gold members get a discount", () => {
  assert.ok(discountCents("gold", 1000) > 0);
});

test("total includes tax", () => {
  assert.equal(totalWithTax(1000), 1000 + taxCents(1000));
});

test("zero quantity is rejected", () => {
  assert.throws(() => parseQuantity("0"));
});

test("charge applies the rate", () => {
  assert.equal(chargeWithAudit(1000, 10), 900);
  assert.equal(chargeWithAudit(500, 20), 400);
});

test("quote for a large bronze order", () => {
  assert.deepEqual(quote(6000, 100), { tier: "bronze", totalCents: 6000 });
});
