import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { notify } from "../src/notify.ts";

test("notify arguments", () => {
  const fn = mock.fn();
  notify(4, { send: fn });
  const calls = fn["mock"].calls; assert.deepEqual(calls[0].arguments, [8]);
});
