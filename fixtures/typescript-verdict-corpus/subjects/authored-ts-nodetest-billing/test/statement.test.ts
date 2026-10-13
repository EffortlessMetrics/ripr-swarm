import test from "node:test";
import { statementTotals } from "../src/statement";

test("statement totals pin the fee", (t) => {
  t.assert.deepStrictEqual(statementTotals(10), { days: 10, fee: 20 });
});
