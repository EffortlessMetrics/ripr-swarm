import { mock, test } from "node:test";
import assert from "node:assert/strict";
import { welcome } from "../src/notify";

test("welcome sends one mail with the exact payload", () => {
  const send = mock.fn();
  welcome({ send }, "ada");
  assert.equal(send.mock.callCount(), 1);
  assert.deepEqual(send.mock.calls[0].arguments, ["ada@example.test", "Welcome, ada"]);
});
