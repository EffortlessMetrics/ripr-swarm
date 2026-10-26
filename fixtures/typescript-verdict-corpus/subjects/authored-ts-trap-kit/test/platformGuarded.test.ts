import { test } from "node:test";
import assert from "node:assert/strict";
import { platformGuarded } from "../src/core.ts";

if (process.platform === "win32") {
  test("platform guarded total", () => {
    assert.equal(platformGuarded(3, 7), 21);
  });
}
