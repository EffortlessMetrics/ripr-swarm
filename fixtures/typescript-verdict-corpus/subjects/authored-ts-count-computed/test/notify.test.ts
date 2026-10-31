import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { notify } from "../src/notify.ts";

test("notify computed", () => {
  const fn = mock.fn();
  notify(4, { send: fn });
  assert.equal(fn["mock"]["calls"]["length"], 1);
});
