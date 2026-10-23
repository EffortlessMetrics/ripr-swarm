import { test } from "node:test";
import assert from "node:assert/strict";
import { isAdmin, isDefaultPort, isHttps, isSaturday, samePair } from "../src/equality.ts";

test("admin role is admin", () => {
  assert.ok(isAdmin("admin"));
  assert.ok(!isAdmin("guest"));
});

test("saturday table", () => {
  const rows: Array<[number, boolean]> = [[0, false], [5, false], [6, true], [13, true]];
  for (const [day, want] of rows) {
    assert.equal(isSaturday(day), want);
  }
});

test("a value matches itself", () => {
  for (let x = 0; x < 5; x++) {
    assert.ok(samePair(x, x));
  }
});

function checkHttps(port: number, want: boolean): void {
  assert.equal(isHttps(port), want);
}

test("https port", () => {
  checkHttps(443, true);
  checkHttps(80, false);
});

test("default port smoke", () => {
  isDefaultPort(8080);
});
