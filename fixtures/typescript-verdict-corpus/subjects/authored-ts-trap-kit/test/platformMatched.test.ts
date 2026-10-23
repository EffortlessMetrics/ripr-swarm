import { test } from "node:test";
import assert from "node:assert/strict";
import { platformMatched } from "../src/core.ts";

if (process.platform !== "win32") {
  test("platform matched total", () => {
    assert.equal(platformMatched(3, 7), 21);
  });
}
