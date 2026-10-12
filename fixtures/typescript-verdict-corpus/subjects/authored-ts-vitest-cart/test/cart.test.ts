import { describe, expect, it, vi } from "vitest";
import { addLine, bulkDiscount, checkout, fetchUnitPrice, isEmpty, lineTotal, removeLine, summary } from "../src/index";

const apple = { sku: "apple", unitCents: 120, quantity: 3 };

describe("cart", () => {
  it("multiplies unit price by quantity", () => {
    expect(lineTotal(apple)).toBe(360);
  });

  it("gives the bulk discount from ten units", () => {
    expect(bulkDiscount(10)).toBe(10);
    expect(bulkDiscount(9)).toBe(0);
  });

  it("rejects a zero quantity with its message", () => {
    expect(() => addLine([], { ...apple, quantity: 0 })).toThrow("quantity must be positive");
  });

  it("refuses to remove a missing line", () => {
    expect(() => removeLine([apple], "pear")).toThrow();
  });

  it("knows a cart with lines is not empty", () => {
    expect(isEmpty([apple])).toBeFalsy();
  });

  it("tells the owner what was charged", () => {
    const send = vi.fn();
    checkout("ann", [apple], { send });
    expect(send).toHaveBeenCalledWith("ann", "charged 360");
  });

  it("looks up a known price", async () => {
    await expect(fetchUnitPrice("pear")).resolves.toBe(95);
  });

  it("summarizes the cart", () => {
    expect(summary([apple, apple])).toMatchInlineSnapshot(`"2 items"`);
  });
});
