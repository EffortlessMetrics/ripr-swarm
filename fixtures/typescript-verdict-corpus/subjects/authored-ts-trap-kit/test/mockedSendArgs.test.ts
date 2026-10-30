import { test } from "node:test";
import assert from "node:assert/strict";
import { mock } from "node:test";
import { mockedSendArgs } from "../src/core.ts";

test("mocked send args", () => {
  const send = mock.fn();
  mockedSendArgs(4, { send });
  assert.deepEqual(send.mock.calls[0].arguments, [8]);
});
