import test from "node:test";
import { graceFee } from "../src/gracefee";

test("grace fee is three per day", (t) => {
  const want = 30;
  t.assert.equal(graceFee(10), want);
});
