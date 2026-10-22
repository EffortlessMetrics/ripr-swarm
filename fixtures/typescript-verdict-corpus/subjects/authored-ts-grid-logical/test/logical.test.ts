import { test } from "node:test";
import assert from "node:assert/strict";
import { canEnter, canShip, isVisible, needsReview, shouldRetry } from "../src/logical.ts";

test("ships only when paid and in stock", () => {
  assert.ok(canShip(true, true));
  assert.ok(!canShip(true, false));
  assert.ok(!canShip(false, true));
});

test("review table", () => {
  const rows: Array<[boolean, boolean, boolean]> = [[false, false, false], [true, false, true], [false, true, true], [true, true, true]];
  for (const [flagged, large, want] of rows) {
    assert.equal(needsReview(flagged, large), want);
  }
});

test("no ticket never enters", () => {
  for (const banned of [true, false]) {
    assert.ok(!canEnter(false, banned));
  }
});

function checkVisible(hidden: boolean, want: boolean): void {
  assert.equal(isVisible(hidden), want);
}

test("visibility", () => {
  checkVisible(true, false);
  checkVisible(false, true);
});

test("retry smoke", () => {
  shouldRetry(true, false);
});
