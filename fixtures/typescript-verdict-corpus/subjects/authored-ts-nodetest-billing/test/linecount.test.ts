import test from "node:test";
import assert from "node:assert";
import { lineCount } from "../src/linecount";

test("line count counts one item", () => {
  assert.equal(lineCount(["a"]), 1);
});
