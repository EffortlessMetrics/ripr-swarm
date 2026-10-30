import { test } from "node:test";
import assert from "node:assert/strict";
import { canRetire, freeShipping, isAdult, overweight, passes } from "../src/boundary.ts";

test("eighteen is adult and seventeen is not", () => {
  assert.ok(isAdult(18));
  assert.ok(!isAdult(17));
});

test("free shipping table", () => {
  const rows: Array<[number, boolean]> = [[0, false], [4999, false], [5000, true], [12000, true]];
  for (const [cents, want] of rows) {
    assert.equal(freeShipping(cents), want);
  }
});

test("retirement is monotonic in age", () => {
  for (let age = 0; age < 120; age++) {
    if (canRetire(age)) {
      assert.ok(canRetire(age + 1));
    }
  }
  assert.ok(!canRetire(0));
  assert.ok(canRetire(119));
});

function checkPass(score: number, want: boolean): void {
  assert.equal(passes(score), want);
}

test("pass mark is fifty", () => {
  checkPass(50, true);
  checkPass(49, false);
});

test("overweight smoke", () => {
  overweight(2500);
});
