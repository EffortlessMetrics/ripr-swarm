import { test } from "node:test";
import assert from "node:assert/strict";
import { filledTable } from "../src/core.ts";

test("filled table", () => {
  const rows: Array<[number, number, number]> = [[3, 7, 21]];
  for (const [a, b, want] of rows) {
    assert.equal(filledTable(a, b), want);
  }
});
