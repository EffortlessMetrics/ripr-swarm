import { checkQuantity } from "../src/quantity";

test("zero quantity is refused with its reason", () => {
  expect(() => checkQuantity(0)).toThrow("quantity must be positive");
});

test("positive quantity passes through", () => {
  expect(checkQuantity(3)).toBe(3);
});
