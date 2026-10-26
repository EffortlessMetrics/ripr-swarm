import { test } from "node:test";
import assert from "node:assert/strict";
import { qualifiesForFreeReturns, qualifiesForFreeShipping, qualifiesForGiftWrap } from "../src/table.ts";

test("free shipping rows", () => {
  const rows: Array<[number, boolean]> = [[0, false], [4999, false], [5000, true], [12000, true]];
  for (const [cents, want] of rows) {
    assert.equal(qualifiesForFreeShipping(cents), want);
  }
});

test("free returns rows sit above the line", () => {
  const rows: Array<[number, boolean]> = [[8500, true], [12000, true], [30000, true]];
  for (const [cents, want] of rows) {
    assert.equal(qualifiesForFreeReturns(cents), want);
  }
});

test("gift wrap rows stop at the first miss", () => {
  const rows: Array<[number, boolean]> = [[0, false], [2999, false], [3000, true], [9000, true]];
  for (const [cents, want] of rows) {
    assert.equal(qualifiesForGiftWrap(cents), want);
    if (!want) {
      break;
    }
  }
});
