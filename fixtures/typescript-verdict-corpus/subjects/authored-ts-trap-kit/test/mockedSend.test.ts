import { test } from "node:test";
import assert from "node:assert/strict";
import { mock } from "node:test";
import { mockedSend } from "../src/core.ts";

test("mocked send", () => {
  const send = mock.fn();
  mockedSend(4, { send });
  assert.equal(send.mock.callCount(), 1);
});
