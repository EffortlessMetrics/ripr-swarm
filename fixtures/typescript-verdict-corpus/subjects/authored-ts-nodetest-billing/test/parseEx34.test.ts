import test from "node:test";
import assert from "node:assert";
import { parseEx34 } from "../src/parseEx34";

test("parseEx34", () => {
  assert.throws(() => parseEx34(""), /^Error: empty$/);
});
