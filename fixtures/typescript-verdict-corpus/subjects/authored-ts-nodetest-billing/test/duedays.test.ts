import test from "node:test";
import { dueDays } from "../src/duedays";

test("due days rejects a non-positive count", (t) => {
  t.assert.throws(() => dueDays(-1), /^Error: days must be positive$/);
});
