import { test } from "node:test";
import assert from "node:assert/strict";
import { emptyTable } from "../src/core.ts";

test("empty table", () => {
  const rows: Array<[number, number, number]> = [];
  for (const [a, b, want] of rows) {
    assert.equal(emptyTable(a, b), want);
  }
});
