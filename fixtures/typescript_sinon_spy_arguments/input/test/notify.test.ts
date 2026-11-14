import sinon from "sinon";
import { test } from "node:test";
import assert from "node:assert/strict";
import { notify } from "../src/notify.ts";

test("notify sinon arguments", () => {
  const fn = sinon.spy();
  notify(4, { send: fn });
  assert.equal(fn.firstCall.args[0], 8);
});
