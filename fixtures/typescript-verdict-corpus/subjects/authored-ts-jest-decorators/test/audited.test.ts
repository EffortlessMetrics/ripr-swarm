import { Ledger } from "../src/audited";

test("the decorated method still adds", () => {
  expect(new Ledger().add(2, 3)).toBe(5);
});
