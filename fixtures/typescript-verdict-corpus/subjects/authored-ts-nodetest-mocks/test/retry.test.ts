import { describe, it, mock } from "node:test";
import assert from "node:assert/strict";
import { withRetry } from "../src/retry";

describe("withRetry", () => {
  it("returns the first success", async (t) => {
    const fn = t.mock.fn(async () => "ok");
    assert.equal(await withRetry(fn, 3), "ok");
    assert.equal(fn.mock.callCount(), 1);
  });

  it("gives up with the last error", async () => {
    const fn = mock.fn(async () => {
      throw new Error("down");
    });
    await assert.rejects(withRetry(fn, 2), /down/);
    assert.equal(fn.mock.callCount(), 2);
  });
});
