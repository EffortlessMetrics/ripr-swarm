import { test } from "node:test";
import assert from "node:assert/strict";
import { floatingPromise } from "../src/core.ts";

test("floating promise", () => {
  floatingPromise(3, 7).then((total) => {
    assert.equal(total, 21);
  });
});
