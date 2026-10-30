import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { describeAttempt, timeoutMs } from "../src/flaky";

describe("timeoutMs", () => {
  it.skip("doubles each attempt", () => {
    assert.equal(timeoutMs(3), 800);
  });

  it.todo("caps the timeout");
});

describe("describeAttempt", () => {
  it("names the first attempt", () => {
    assert.equal(describeAttempt(0), "first");
  });

  it("skips from inside the test", (t) => {
    t.skip("not ready");
    assert.equal(describeAttempt(2), "retry 2");
  });
});
