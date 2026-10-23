import { test } from "node:test";
import assert from "node:assert/strict";
import { commission, handling, lateFee, roundingUnit, salesTax } from "../src/constant.ts";

test("tax on a thousand", () => {
  assert.equal(salesTax(1000), 80);
});

test("late fee table", () => {
  const rows: Array<[number, number]> = [[0, 0], [1, 5], [4, 20]];
  for (const [days, want] of rows) {
    assert.equal(lateFee(days), want);
  }
});

test("commission stays within the sale", () => {
  for (let sale = 0; sale < 1000; sale += 100) {
    const value = commission(sale);
    assert.ok(value >= 0 && value <= sale);
  }
});

function checkHandling(weight: number, want: number): void {
  assert.equal(handling(weight), want);
}

test("handling", () => {
  checkHandling(2, 24);
});

test("rounding smoke", () => {
  roundingUnit(37);
});
