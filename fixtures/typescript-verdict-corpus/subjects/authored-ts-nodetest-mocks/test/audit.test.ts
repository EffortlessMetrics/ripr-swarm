import { mock, test } from "node:test";
import assert from "node:assert/strict";
import { record, sink } from "../src/audit";

test("record writes once through the sink", () => {
  const write = mock.method(sink, "write", () => undefined);
  record("login");
  assert.equal(write.mock.callCount(), 1);
});
