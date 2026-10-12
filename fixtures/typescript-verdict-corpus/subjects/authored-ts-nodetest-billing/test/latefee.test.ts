import test from "node:test";
import { lateFee } from "../src/latefee";

test("late fee is two per day", (t) => {
  t.assert.equal(lateFee(10), 20);
});
