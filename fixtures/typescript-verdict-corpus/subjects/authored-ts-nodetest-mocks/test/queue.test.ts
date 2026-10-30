import { beforeEach, describe, it } from "node:test";
import assert from "node:assert/strict";
import { Queue } from "../src/queue";

describe("Queue", () => {
  let queue: Queue<string>;

  beforeEach(() => {
    queue = new Queue<string>();
  });

  it("push returns the new length", () => {
    queue.push("a");
    assert.equal(queue.push("b"), 2);
  });

  it("shift takes from the front", () => {
    queue.push("a");
    queue.push("b");
    assert.equal(queue.shift(), "a");
  });
});
