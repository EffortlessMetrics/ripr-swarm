import assert from "node:assert";
import test from "node:test";
import { tally } from "../src/tally";

test("tally counts one item", () => {
  assert.equal(tally(["a"]), 1);
});
