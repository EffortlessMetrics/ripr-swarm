import { test } from "node:test";
import assert from "node:assert/strict";
import { awaitedPromise } from "../src/core.ts";

test("awaited promise", async () => {
  assert.equal(await awaitedPromise(3, 7), 21);
});
