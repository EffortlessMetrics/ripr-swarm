import { expect, test } from "vitest";
import { orderTotal } from "../src/total";

test("total is positive", () => {
  expect(orderTotal(2, 10, 5)).toBeGreaterThan(0);
});
