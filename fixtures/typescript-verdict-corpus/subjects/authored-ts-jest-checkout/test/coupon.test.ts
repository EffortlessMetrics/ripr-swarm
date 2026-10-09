import { parseCoupon } from "../src/coupon";

test("a foreign coupon is refused", () => {
  expect(() => parseCoupon("FREE10")).toThrow();
});

test("a SAVE coupon yields its amount", () => {
  expect(parseCoupon("SAVE15")).toBe(15);
});
