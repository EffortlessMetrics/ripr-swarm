import test from "node:test";
import { creditNote } from "../src/creditnote";

test("credit note", async (t) => {
  await t.test("takes five off the amount", (t2) => {
    t2.assert.strictEqual(creditNote(30), 25);
  });
});
