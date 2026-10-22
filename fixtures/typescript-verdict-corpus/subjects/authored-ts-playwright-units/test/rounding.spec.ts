import { expect, test } from "@playwright/test";
import { toCents } from "../src/rounding";

const rows: Array<[number, number]> = [
  [1, 100],
  [2.5, 250],
];

for (const [amount, cents] of rows) {
  test(`converts ${amount}`, () => {
    expect(toCents(amount)).toBe(cents);
  });
}
