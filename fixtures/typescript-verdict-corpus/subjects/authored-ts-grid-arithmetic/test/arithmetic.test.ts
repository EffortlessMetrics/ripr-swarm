import { test } from "node:test";
import assert from "node:assert/strict";
import { area, lineTotal, scaled, seconds, tip } from "../src/arithmetic.ts";

test("three at seven costs twenty one", () => {
  assert.equal(lineTotal(3, 7), 21);
});

test("area table", () => {
  const rows: Array<[number, number, number]> = [[2, 3, 6], [4, 5, 20], [1, 9, 9]];
  for (const [width, height, want] of rows) {
    assert.equal(area(width, height), want);
  }
});

test("scaling by one keeps the quantity", () => {
  for (let unit = 1; unit < 5; unit++) {
    assert.equal(scaled(1, unit), unit);
  }
});

function checkTip(bill: number, rate: number, want: number): void {
  assert.equal(tip(bill, rate), want);
}

test("tip", () => {
  checkTip(40, 2, 80);
});

test("seconds smoke", () => {
  seconds(3);
});
