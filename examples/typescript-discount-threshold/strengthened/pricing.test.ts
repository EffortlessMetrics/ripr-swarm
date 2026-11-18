import { describe, expect, it } from "vitest";

import { discountedTotal } from "../src/pricing";

describe("discountedTotal", () => {
  it("leaves a below-threshold subtotal unchanged", () => {
    expect(discountedTotal(9999)).toBe(9999);
  });

  it("discounts an above-threshold subtotal", () => {
    expect(discountedTotal(10001)).toBe(9501);
  });

  it("discounts a subtotal exactly at the threshold", () => {
    expect(discountedTotal(10000)).toBe(9500);
  });
});
