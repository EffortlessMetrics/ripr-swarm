import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { notify } from "../src/notify.ts";

test("notify alias", () => {
  const fn = mock.fn();
  notify(4, { send: fn });
  const calls = fn.mock.calls; assert.equal(calls.length, 1);
});
