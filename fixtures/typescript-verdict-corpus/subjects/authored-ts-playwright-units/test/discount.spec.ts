import { expect, test } from "@playwright/test";
import { discount } from "../src/discount";

test.describe("discount", () => {
  test("ten units get the bulk discount", () => {
    expect(discount(10)).toBe(10);
  });

  test("nine units get nothing", () => {
    expect(discount(9)).toBe(0);
  });
});
