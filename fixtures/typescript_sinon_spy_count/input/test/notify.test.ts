import sinon from "sinon";
import { test } from "node:test";
import assert from "node:assert/strict";
import { notify } from "../src/notify.ts";

test("notify sinon count", () => {
  const fn = sinon.spy();
  notify(4, { send: fn });
  assert.equal(fn.callCount, 1);
});
