import { invoiceTotal } from "../src/invoice";
import { salesTax } from "../src/tax";

jest.mock("../src/tax", () => ({ salesTax: jest.fn(() => 5) }));

test("invoice adds the tax the tax module reports", () => {
  expect(invoiceTotal(100)).toBe(105);
  expect(salesTax).toHaveBeenCalledWith(100);
});
